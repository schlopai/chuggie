// Headless GBA screenshot renderer. Loads a ROM into libmgba, runs N frames with NO display
// (no window, no Screen-Recording/Accessibility permission), and writes the final 240x160
// framebuffer as a binary PPM (P6). The wrapper `scripts/screenshot.sh` converts it to PNG.
// This is the headless path the CI plan calls for — see README.
//
// Optional 4th arg: held keys for the whole run, OR a frame schedule:
//   "a,b"           — hold those keys every frame
//   "0x80"          — hex mask held every frame
//   "90:a,120:"     — no keys until frame 90, hold A until 120, then none
//                     (comma-separated frame:keys entries; empty keys = release; up to MAX_SCHED)
//
// Set GBA_SHOT_LOG=1 to forward the ROM's `log()` output to stderr, prefixed with the frame it
// happened on — the way to measure load times and input→pixel latency against a frame budget.
//
// Set GBA_SHOT_TRACE=1 to report every frame on which the picture changed, as
// `[frame N] screen 0xHASH (M px non-white)`. That is the objective measure of a load or menu
// budget: it says which frame the player first saw the new screen, with no ROM instrumentation.
//
// Set GBA_SHOT_AUDIO=<out.wav> to capture the emulated sound to a 16-bit stereo WAV, and a
// one-line `audio:` summary (samples, peak, RMS) to stderr. This is to sound what the framebuffer
// is to picture: it makes "does it actually play, and what note" a thing a harness can assert
// rather than a thing someone has to put headphones on for. Both the software mixer (Direct Sound)
// and the PSG/DMG channels land in the same capture, so it also shows the two coexisting.
//
// Build:  cc tools/gba-shot.c -o tools/gba-shot -I<mgba>/include -L<mgba>/lib -lmgba
//         (scripts/screenshot.sh discovers <mgba> via brew/apt/MGBA_PREFIX.)
#include <mgba/core/blip_buf.h>
#include <mgba/core/core.h>
#include <mgba/core/interface.h>
#include <mgba/core/log.h>
// Internal headers, for the CPU state behind a bad-memory log line. See `g_core` below.
#include <mgba/internal/arm/arm.h>
#include <mgba/internal/gba/gba.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// GBA keypad bits (standard REG_KEYINPUT layout, matching mGBA's GBAKey enum).
static const struct { const char* name; unsigned bit; } KEYS[] = {
    {"a", 0x001}, {"b", 0x002}, {"select", 0x004}, {"start", 0x008},
    {"right", 0x010}, {"left", 0x020}, {"up", 0x040}, {"down", 0x080},
    {"r", 0x100}, {"l", 0x200},
};

static unsigned parse_keys(const char* s) {
    if (!s || !*s) return 0;
    if (s[0] == '0' && (s[1] == 'x' || s[1] == 'X')) return (unsigned)strtoul(s, NULL, 16);
    unsigned mask = 0;
    char buf[128];
    strncpy(buf, s, sizeof buf - 1);
    buf[sizeof buf - 1] = '\0';
    // strtok_r, not strtok: this is called from inside parse_schedule's own strtok loop over a
    // DIFFERENT buffer — plain strtok shares one hidden static cursor, so the nested call here
    // would silently clobber the outer loop's position and truncate every schedule after entry 1.
    char* saveptr = NULL;
    for (char* tok = strtok_r(buf, ",", &saveptr); tok; tok = strtok_r(NULL, ",", &saveptr)) {
        for (size_t i = 0; i < sizeof KEYS / sizeof KEYS[0]; i++) {
            if (strcmp(tok, KEYS[i].name) == 0) { mask |= KEYS[i].bit; break; }
        }
    }
    return mask;
}

// Schedule: up to MAX_SCHED (start_frame, keys) pairs. keys apply from that frame until the next
// entry. Overflow is REPORTED, not silently dropped: a truncated schedule looks exactly like a ROM
// that ignores input, which is a trap when scripting long menu walkthroughs.
#define MAX_SCHED 4096
static int sched_n = 0;
static int sched_at[MAX_SCHED];
static unsigned sched_keys[MAX_SCHED];
static int is_schedule = 0;

static int parse_schedule(const char* s) {
    // Schedule if any token contains ':'
    if (!s || !strchr(s, ':')) return 0;
    // Room for a full MAX_SCHED schedule: a 256-byte buffer silently truncated a long one, so the
    // late entries (the presses a harness actually cares about) never ran. Same trap at the entry limit:
    // a run that stops taking input at frame 1200 sits idle for the rest of a "10,000 frame" test and
    // reports clean, so the limit is now larger than any walkthrough anyone has scripted.
    char buf[65536];
    if (strlen(s) >= sizeof buf) {
        fprintf(stderr, "gba-shot: schedule longer than %zu chars — truncated\n", sizeof buf - 1);
    }
    strncpy(buf, s, sizeof buf - 1);
    buf[sizeof buf - 1] = '\0';
    sched_n = 0;
    int dropped = 0;
    // strtok_r (see parse_keys) — this loop calls parse_keys() per-token, which itself tokenizes.
    char* saveptr = NULL;
    for (char* tok = strtok_r(buf, ",", &saveptr); tok; tok = strtok_r(NULL, ",", &saveptr)) {
        char* colon = strchr(tok, ':');
        if (!colon) continue;
        if (sched_n >= MAX_SCHED) { dropped++; continue; }
        *colon = '\0';
        sched_at[sched_n] = atoi(tok);
        sched_keys[sched_n] = parse_keys(colon + 1);
        sched_n++;
    }
    if (dropped) {
        fprintf(stderr, "gba-shot: schedule has more than %d entries — dropped the last %d\n",
                MAX_SCHED, dropped);
    }
    return sched_n > 0;
}

static unsigned keys_for_frame(int frame, unsigned held) {
    if (!is_schedule) return held;
    unsigned k = 0;
    for (int i = 0; i < sched_n; i++) {
        if (frame >= sched_at[i]) k = sched_keys[i];
    }
    return k;
}

// Frame currently being run, so forwarded log lines can be timestamped (see null_log).
static int cur_frame = 0;

// The running core, so `null_log` can report WHERE a bad memory access came from.
//
// mGBA logs "Bad memory Store32: 0xADDR" with no clue as to the culprit, which makes a corrupted
// ROM a guessing game — the address alone sent this project chasing the wrong subsystem more than
// once. The PC and LR at the moment of the access turn it into an `addr2line` lookup against the
// ELF that produced the ROM:
//
//   arm-none-eabi-addr2line -f -C -e .tish/gba/<name>/target/*/release/<name> <pc>
//
// Only attached when GBA_SHOT_PC is set, because reading CPU state per log line is not free and
// most runs do not care.
static struct mCore* g_core = NULL;

// ── audio capture (GBA_SHOT_AUDIO) ───────────────────────────────────────────────────────────────
// mGBA resamples into two blip buffers (0 = left, 1 = right); we drain them after every frame and
// stream 16-bit stereo PCM straight out. The 44-byte RIFF header is written up front with zeroed
// sizes and patched on close, so the capture never has to be held in memory.
#define AUDIO_RATE 44100
#define AUDIO_CHUNK 2048

static FILE* audio_file = NULL;
static unsigned long audio_frames = 0;  // sample frames (L+R pairs) written
static int audio_peak = 0;              // |sample| max, to report silence vs. sound
static double audio_sq = 0;             // running sum of squares, for RMS

static void put_u32(FILE* f, unsigned v) { fputc(v & 0xFF, f); fputc((v >> 8) & 0xFF, f); fputc((v >> 16) & 0xFF, f); fputc((v >> 24) & 0xFF, f); }
static void put_u16(FILE* f, unsigned v) { fputc(v & 0xFF, f); fputc((v >> 8) & 0xFF, f); }

static void audio_open(const char* path) {
    audio_file = fopen(path, "wb");
    if (!audio_file) { fprintf(stderr, "gba-shot: cannot open %s for audio\n", path); return; }
    fwrite("RIFF", 1, 4, audio_file); put_u32(audio_file, 0);      // patched on close
    fwrite("WAVEfmt ", 1, 8, audio_file); put_u32(audio_file, 16);
    put_u16(audio_file, 1);                                        // PCM
    put_u16(audio_file, 2);                                        // stereo
    put_u32(audio_file, AUDIO_RATE);
    put_u32(audio_file, AUDIO_RATE * 2 * 2);                       // byte rate
    put_u16(audio_file, 4);                                        // block align
    put_u16(audio_file, 16);                                       // bits
    fwrite("data", 1, 4, audio_file); put_u32(audio_file, 0);      // patched on close
}

static void audio_drain(struct mCore* core) {
    if (!audio_file) return;
    blip_t* left = core->getAudioChannel(core, 0);
    blip_t* right = core->getAudioChannel(core, 1);
    if (!left || !right) return;
    for (int avail = blip_samples_avail(left); avail > 0; avail = blip_samples_avail(left)) {
        int n = avail > AUDIO_CHUNK ? AUDIO_CHUNK : avail;
        static short buf[AUDIO_CHUNK * 2];
        // stereo=1 makes each read stride by 2, so left lands on the even slots and right on the odd
        // ones and the buffer comes out already interleaved.
        blip_read_samples(left, buf, n, 1);
        blip_read_samples(right, buf + 1, n, 1);
        for (int i = 0; i < n * 2; i++) {
            int s = buf[i];
            int mag = s < 0 ? -s : s;
            if (mag > audio_peak) audio_peak = mag;
            audio_sq += (double)s * (double)s;
        }
        fwrite(buf, sizeof(short), (size_t)n * 2, audio_file);
        audio_frames += (unsigned long)n;
    }
}

static void audio_close(void) {
    if (!audio_file) return;
    unsigned long data_bytes = audio_frames * 4;
    fseek(audio_file, 4, SEEK_SET);  put_u32(audio_file, (unsigned)(36 + data_bytes));
    fseek(audio_file, 40, SEEK_SET); put_u32(audio_file, (unsigned)data_bytes);
    fclose(audio_file);
    audio_file = NULL;
    double rms = audio_frames ? sqrt(audio_sq / (double)(audio_frames * 2)) : 0.0;
    fprintf(stderr, "gba-shot: audio %lu samples (%.2fs) peak %d rms %.1f%s\n",
            audio_frames, (double)audio_frames / AUDIO_RATE, audio_peak, rms,
            audio_peak == 0 ? "  — SILENT" : "");
}

// Swallow mGBA's internal BIOS/DMA/etc. logging so only the framebuffer + our status line show.
// If GBA_SHOT_LOG is set in the environment, forward log lines to stderr instead (so a ROM's
// `agb::println!`/tish `log()` output — e.g. timing instrumentation — is visible to the harness).
// Each line is prefixed with `[frame N]`: at 59.7fps that is the wall clock a player sees, which
// is what makes this usable for load/latency budgets and not just printf debugging.
static void null_log(struct mLogger* l, int cat, enum mLogLevel level, const char* fmt, va_list a) {
    (void)l; (void)cat; (void)level;
    if (getenv("GBA_SHOT_LOG")) {
        fprintf(stderr, "[frame %d] ", cur_frame);
        if (g_core && getenv("GBA_SHOT_PC")
            && (strstr(fmt, "Bad memory") || strstr(fmt, "Jumped to invalid")
                || strstr(fmt, "Illegal opcode") || strstr(fmt, "Unimplemented memory")
                // With GBA_SHOT_PC set, SWI trace lines carry pc/lr/sp too: a ROM wedged in a
                // silent halt loop shows only repeating SWIs, and the pc/sp name the loop.
                || strstr(fmt, "SWI"))) {
            struct ARMCore* cpu = ((struct GBA*) g_core->board)->cpu;
            fprintf(stderr, "[pc=%08X lr=%08X sp=%08X] ", cpu->gprs[15], cpu->gprs[14],
                    cpu->gprs[13]);
        }
        vfprintf(stderr, fmt, a);
        fputc('\n', stderr);
    }
}

int main(int argc, char** argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s <rom.gba> <out.ppm> [frames] [held-keys|frame:keys,...]\n", argv[0]);
        return 2;
    }
    const char* rom = argv[1];
    const char* out = argv[2];
    int frames = argc > 3 ? atoi(argv[3]) : 60;
    unsigned keys = 0;
    if (argc > 4) {
        is_schedule = parse_schedule(argv[4]);
        if (!is_schedule) keys = parse_keys(argv[4]);
    }

    struct mCore* core = mCoreFind(rom);
    if (!core) { fprintf(stderr, "gba-shot: no emulator core for %s\n", rom); return 1; }
    core->init(core);
    g_core = core;   // for null_log's optional pc= report
    mCoreInitConfig(core, "gba-shot");

    // Logger AFTER mCoreInitConfig: loading the config rewrites the active log filter, which
    // silently dropped the gba.debug channel that agb's `println!` (tish `log()`) writes to.
    static struct mLogger logger;
    static struct mLogFilter filter;
    mLogFilterInit(&filter);
    filter.defaultLevels = mLOG_ALL;
    mLogFilterSet(&filter, "gba.debug", mLOG_ALL);
    logger.log = null_log;
    logger.filter = &filter;
    mLogSetDefaultLogger(&logger);

    unsigned w = 0, h = 0;
    core->desiredVideoDimensions(core, &w, &h);
    color_t* buffer = calloc((size_t)w * h, BYTES_PER_PIXEL);
    core->setVideoBuffer(core, buffer, w);

    if (!mCoreLoadFile(core, rom)) { fprintf(stderr, "gba-shot: failed to load %s\n", rom); return 1; }

    // Attach the cartridge save file, so SRAM survives between runs.
    //
    // Without this the core keeps save data in memory and drops it on exit, which makes a headless
    // "write it, power-cycle, read it back" test impossible — the second run always sees a blank
    // cart and every save looks like it silently failed. GBA_SHOT_NOSAVE=1 restores the old
    // throwaway behaviour for tests that want a guaranteed-fresh cartridge.
    if (!getenv("GBA_SHOT_NOSAVE")) {
        mCoreAutoloadSave(core);
    }
    core->reset(core);

    // Audio rates must be set AFTER reset — reset reinitialises the core's audio, which would
    // otherwise leave the blip buffers resampling to mGBA's configured rate instead of ours.
    const char* audio_out = getenv("GBA_SHOT_AUDIO");
    if (audio_out && *audio_out) {
        audio_open(audio_out);
        core->setAudioBufferSize(core, AUDIO_CHUNK);
        blip_set_rates(core->getAudioChannel(core, 0), core->frequency(core), AUDIO_RATE);
        blip_set_rates(core->getAudioChannel(core, 1), core->frequency(core), AUDIO_RATE);
    }
    // Re-assert keys BEFORE each frame — mGBA samples the key state per frame.
    const int trace = getenv("GBA_SHOT_TRACE") != NULL;
    unsigned prev_hash = 0;
    for (int i = 0; i < frames; i++) {
        cur_frame = i;
        unsigned k = keys_for_frame(i, keys);
        if (k) core->setKeys(core, k);
        else core->setKeys(core, 0);
        core->runFrame(core);
        audio_drain(core);
        if (trace) {
            // FNV-1a over the framebuffer, plus a non-white count so a forced-blank/white screen
            // is distinguishable from real picture at a glance.
            unsigned hash = 2166136261u, painted = 0;
            for (size_t p = 0; p < (size_t)w * h; p++) {
                color_t c = buffer[p];
                if ((c & 0xFFFFFF) != 0xFFFFFF) painted++;
                hash = (hash ^ (unsigned)(c & 0xFFFFFF)) * 16777619u;
            }
            if (hash != prev_hash) {
                fprintf(stderr, "[frame %d] screen 0x%08X (%u px painted)\n", i, hash, painted);
                prev_hash = hash;
            }
        }
    }

    FILE* f = fopen(out, "wb");
    if (!f) { fprintf(stderr, "gba-shot: cannot open %s for writing\n", out); return 1; }
    fprintf(f, "P6\n%u %u\n255\n", w, h);
    for (size_t i = 0; i < (size_t)w * h; i++) {
        color_t c = buffer[i];                              // native: 0xAABBGGRR (byte0=R,1=G,2=B)
        unsigned char px[3] = { (unsigned char)(c & 0xFF),
                                (unsigned char)((c >> 8) & 0xFF),
                                (unsigned char)((c >> 16) & 0xFF) };
        fwrite(px, 1, 3, f);
    }
    fclose(f);
    free(buffer);
    audio_close();
    // Flush save data before tearing the core down; unloading is what commits it to disk.
    if (!getenv("GBA_SHOT_NOSAVE")) {
        core->unloadROM(core);
    }
    core->deinit(core);
    fprintf(stderr, "gba-shot: rendered %ux%u @ frame %d%s\n", w, h, frames,
            is_schedule ? " (key schedule)" : (keys ? " (keys held)" : ""));
    return 0;
}
