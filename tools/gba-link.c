// Headless TWO-CONSOLE link test. Runs two GBA cores in one process with their serial ports wired
// together through mGBA's own SIO lockstep, so a link-cable game can be exercised end to end
// without hardware.
//
// WHY THIS EXISTS. `tools/gba-shot` runs one core, which means a link game always sees an empty
// cable: the SIO registers read "nothing attached", the game takes its offline path, and the whole
// transport — the register writes, the master/child split, the transfer handshake — is never
// executed by any test. That is a large piece of code to leave entirely unproven, and it is the
// piece that is hardest to debug on hardware, where the only instrument is a screen.
//
// mGBA models a cable as a `GBASIOLockstep` with one `GBASIOLockstepNode` per console. Attach a
// node to each core's multiplayer driver slot and the cores negotiate ids, mark themselves ready,
// and move words exactly as two consoles would. Stepping both in the same thread is fine because
// the lockstep is what synchronises them.
//
// Usage:
//   gba-link <rom0> <rom1> <out0.ppm> <out1.ppm> <frames> [keys0] [keys1]
//
// Keys use the same syntax as gba-shot: held keys ("a,start") or a frame schedule ("90:a,120:").
// GBA_LINK_LOG=1 prints each console's `log()` output prefixed with its id.
//
// Build: cc tools/gba-link.c -o tools/gba-link -I<mgba>/include -L<mgba>/lib -lmgba
#include <mgba/core/core.h>
#include <mgba/core/log.h>
#include <mgba/internal/gba/gba.h>
#include <mgba/internal/gba/sio.h>
#include <mgba/internal/gba/sio/lockstep.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CONSOLES 2
#define MAX_STEPS 256

struct sched {
    int at[MAX_STEPS];
    unsigned keys[MAX_STEPS];
    int n;
    int held;      // 1 when the argument was a plain held-key list rather than a schedule
    unsigned flat;
};

// Log lines are stamped with the LOGGING core's own frame counter, read live. One shared counter
// (updated from core 0 only) stamped core 1's lines with core 0's frame: the cores interleave in
// 128-instruction slices, so whenever core 1 logged after core 0 had rolled into the next frame,
// its line was tagged one frame high — which read as a 1-frame "desync" in event logs that were
// in perfect lockstep.
static struct mCore* log_cores[MAX_GBAS];
static int cur_core = 0;

// THE CYCLE BANK. `mLockstepInit` leaves the coordination callbacks NULL — mGBA's own frontends
// fill them with a threaded implementation, and only `lock`/`unlock` are null-checked before use.
// The first transfer therefore calls a NULL `useCycles` and the process dies with SIGSEGV, which
// is what happened before this existed.
//
// Both cores run in ONE thread here, alternating whole frames, so there is nothing to block on:
// `wait` and `signal` are trivially true, and the only real bookkeeping is how many cycles each
// node has banked. That is enough because the lockstep still gates the TRANSFER itself — the
// consoles cannot exchange a word until both have reached the point of asking for one.
static int32_t bank[MAX_GBAS];

static void ls_add_cycles(struct mLockstep* l, int id, int32_t cycles) {
    (void)l;
    if (id >= 0 && id < MAX_GBAS) bank[id] += cycles;
}

static int32_t ls_use_cycles(struct mLockstep* l, int id, int32_t cycles) {
    (void)l;
    if (id < 0 || id >= MAX_GBAS) return 0;
    bank[id] -= cycles;
    if (bank[id] < 0) bank[id] = 0;
    return bank[id];
}

static int32_t ls_unused_cycles(struct mLockstep* l, int id) {
    (void)l;
    return (id >= 0 && id < MAX_GBAS) ? bank[id] : 0;
}

static bool ls_signal(struct mLockstep* l, unsigned mask) { (void)l; (void)mask; return true; }
static bool ls_wait(struct mLockstep* l, unsigned mask) { (void)l; (void)mask; return true; }
static void ls_unload(struct mLockstep* l, int id) { (void)l; if (id >= 0 && id < MAX_GBAS) bank[id] = 0; }

static void link_log(struct mLogger* l, int cat, enum mLogLevel level, const char* fmt, va_list a) {
    (void)l; (void)cat; (void)level;
    if (getenv("GBA_LINK_LOG")) {
        struct mCore* c = log_cores[cur_core];
        fprintf(stderr, "[p%d frame %d] ", cur_core, c ? (int) c->frameCounter(c) : 0);
        vfprintf(stderr, fmt, a);
        fputc('\n', stderr);
    }
}

static unsigned key_bit(const char* name, size_t len) {
    // Same codes gba-shot uses, which are the GBA's own key bit order.
    if (!strncmp(name, "a", len) && len == 1) return 1 << 0;
    if (!strncmp(name, "b", len) && len == 1) return 1 << 1;
    if (!strncmp(name, "select", len) && len == 6) return 1 << 2;
    if (!strncmp(name, "start", len) && len == 5) return 1 << 3;
    if (!strncmp(name, "right", len) && len == 5) return 1 << 4;
    if (!strncmp(name, "left", len) && len == 4) return 1 << 5;
    if (!strncmp(name, "up", len) && len == 2) return 1 << 6;
    if (!strncmp(name, "down", len) && len == 4) return 1 << 7;
    if (!strncmp(name, "r", len) && len == 1) return 1 << 8;
    if (!strncmp(name, "l", len) && len == 1) return 1 << 9;
    return 0;
}

static unsigned parse_keys(const char* s) {
    unsigned out = 0;
    while (*s) {
        const char* e = strchr(s, ',');
        size_t len = e ? (size_t)(e - s) : strlen(s);
        if (len) out |= key_bit(s, len);
        if (!e) break;
        s = e + 1;
    }
    return out;
}

// "frame:keys,frame:keys" — keys are HELD from that frame until the next entry, so every press
// needs an explicit release ("300:right,308:").
static void parse_sched(const char* s, struct sched* out) {
    memset(out, 0, sizeof(*out));
    if (!s || !*s) return;
    if (!strchr(s, ':')) {
        out->held = 1;
        out->flat = parse_keys(s);
        return;
    }
    while (*s && out->n < MAX_STEPS) {
        char* end = NULL;
        long at = strtol(s, &end, 10);
        if (end == s || *end != ':') break;
        s = end + 1;
        const char* comma = strchr(s, ',');
        size_t len = comma ? (size_t)(comma - s) : strlen(s);
        char buf[128];
        if (len >= sizeof(buf)) len = sizeof(buf) - 1;
        memcpy(buf, s, len);
        buf[len] = 0;
        out->at[out->n] = (int)at;
        out->keys[out->n] = len ? parse_keys(buf) : 0;
        out->n++;
        if (!comma) break;
        s = comma + 1;
    }
}

static unsigned sched_at(const struct sched* s, int frame) {
    if (s->held) return s->flat;
    unsigned k = 0;
    for (int i = 0; i < s->n && s->at[i] <= frame; ++i) k = s->keys[i];
    return k;
}

static void write_ppm(const char* path, const color_t* px, unsigned w, unsigned h) {
    FILE* f = fopen(path, "wb");
    if (!f) return;
    fprintf(f, "P6\n%u %u\n255\n", w, h);
    for (unsigned i = 0; i < w * h; ++i) {
        color_t c = px[i];
        unsigned char rgb[3] = { (unsigned char)(c & 0xFF),
                                 (unsigned char)((c >> 8) & 0xFF),
                                 (unsigned char)((c >> 16) & 0xFF) };
        fwrite(rgb, 1, 3, f);
    }
    fclose(f);
}

int main(int argc, char** argv) {
    if (argc < 6) {
        fprintf(stderr,
            "usage: gba-link <rom0> <rom1> <out0.ppm> <out1.ppm> <frames> [keys0] [keys1]\n");
        return 2;
    }
    const char* roms[CONSOLES] = { argv[1], argv[2] };
    const char* outs[CONSOLES] = { argv[3], argv[4] };
    int frames = atoi(argv[5]);
    struct sched sch[CONSOLES];
    parse_sched(argc > 6 ? argv[6] : "", &sch[0]);
    parse_sched(argc > 7 ? argv[7] : "", &sch[1]);

    static struct mLogger logger;
    static struct mLogFilter filter;
    mLogFilterInit(&filter);
    filter.defaultLevels = mLOG_ALL;
    mLogFilterSet(&filter, "gba.debug", mLOG_ALL);
    logger.log = link_log;
    logger.filter = &filter;

    // THE CABLE. One lockstep, one node per console; the node is installed in each core's
    // MULTIPLAYER driver slot, which is the slot the game selects when it writes SIOCNT bit 13.
    static struct GBASIOLockstep lockstep;
    static struct GBASIOLockstepNode nodes[CONSOLES];
    GBASIOLockstepInit(&lockstep);
    lockstep.d.addCycles = ls_add_cycles;
    lockstep.d.useCycles = ls_use_cycles;
    lockstep.d.unusedCycles = ls_unused_cycles;
    lockstep.d.signal = ls_signal;
    lockstep.d.wait = ls_wait;
    lockstep.d.unload = ls_unload;

    struct mCore* cores[CONSOLES];
    color_t* fb[CONSOLES];
    unsigned w = 0, h = 0;

    for (int i = 0; i < CONSOLES; ++i) {
        cores[i] = mCoreFind(roms[i]);
        if (!cores[i]) { fprintf(stderr, "gba-link: no core for %s\n", roms[i]); return 1; }
        log_cores[i] = cores[i];
        cores[i]->init(cores[i]);
        mCoreInitConfig(cores[i], "gba-link");
        // The logger must be set AFTER mCoreInitConfig, which rewrites the active filter and
        // otherwise silently drops the gba.debug channel that tish `log()` writes to.
        mLogSetDefaultLogger(&logger);
        cores[i]->desiredVideoDimensions(cores[i], &w, &h);
        fb[i] = calloc((size_t)w * h, BYTES_PER_PIXEL);
        cores[i]->setVideoBuffer(cores[i], fb[i], w);
        if (!mCoreLoadFile(cores[i], roms[i])) {
            fprintf(stderr, "gba-link: failed to load %s\n", roms[i]);
            return 1;
        }
        cores[i]->reset(cores[i]);

        GBASIOLockstepNodeCreate(&nodes[i]);
        if (!GBASIOLockstepAttachNode(&lockstep, &nodes[i])) {
            fprintf(stderr, "gba-link: could not attach console %d to the cable\n", i);
            return 1;
        }
        struct GBA* gba = (struct GBA*) cores[i]->board;
        struct GBASIODriverSet set = { 0 };
        set.multiplayer = &nodes[i].d;
        GBASIOSetDriverSet(&gba->sio, &set);
    }

    // INTERLEAVE IN SMALL SLICES, not whole frames.
    //
    // A serial transfer only completes when both consoles have reached the point of taking part in
    // it, and `mLockstep`'s coordination callbacks are written for two threads that block on each
    // other. In one thread they cannot block — so instead the cores are advanced a few hundred
    // instructions at a time, which brings them to the transfer close enough together that the
    // lockstep's own cycle accounting resolves it.
    //
    // Running whole frames looked like it worked and did not: the master started a transfer, ran
    // out its entire frame with the child not yet stepped, and the transfer never completed. The
    // symptom was a link that reached MULTI mode and then went silent, which is indistinguishable
    // from a protocol bug and cost a while to pin on the harness.
    const int SLICE = 128;
    uint32_t counted[CONSOLES] = { 0 };
    int done = 0;
    while (!done) {
        for (int i = 0; i < CONSOLES; ++i) {
            cur_core = i;
            uint32_t before = cores[i]->frameCounter(cores[i]);
            cores[i]->setKeys(cores[i], sched_at(&sch[i], (int) before));
            for (int s = 0; s < SLICE; ++s) {
                cores[i]->step(cores[i]);
            }
            uint32_t after = cores[i]->frameCounter(cores[i]);
            if (after != before) {
                counted[i] = after;
            }
        }
        done = 1;
        for (int i = 0; i < CONSOLES; ++i) {
            if ((int) counted[i] < frames) done = 0;
        }
    }

    for (int i = 0; i < CONSOLES; ++i) {
        write_ppm(outs[i], fb[i], w, h);
        cores[i]->deinit(cores[i]);
    }
    fprintf(stderr, "gba-link: ran %d frames on %d consoles\n", frames, CONSOLES);
    return 0;
}
