# packages/clan — FFTA clan runtime

Mutable clan state (gil, bag, roster, equip, AP/abilities, formation) plus curated tables
codegen’d from the offline archive at [`data/ffta/`](../../data/ffta/).

| File | Role |
|------|------|
| `state.tish` | Gil, CP, bag, roster, equip, jobs/abilities APIs; shop adapters |
| `party.tish` | Start → Party / Clan / Link / Area List / System UI (`packages/ui`) |
| `*_gen.tish` | Allowlisted jobs/items/abilities — **not** the full CSV |

## Town backlog

Living requirements, acceptance criteria, and phase status:
**[`docs/ffta-clan-menu.md`](../../docs/ffta-clan-menu.md)**.

## FFTA mapping

| FFTA | This package |
|------|----------------|
| Gil | `clanGil` / shop `cur: "Gil"` |
| CP | `clanCp` (display + earn on buy) |
| Party roster | `clanRoster` units |
| Equip Items | `party.tish` Equip screen + `clanCanEquip` |
| Pick Abilities / Change Jobs | unit submenu + state APIs |
| Item List | bag by category; Use on consumables |
| Formation | `clanFormation` for future tactics deploy |
| JP | battle-only (Judge Points) — see `data/ffta/tables/currencies.md` |

## Regenerate tables

```bash
python3 scripts/ffta_gen_clan_tables.py
```

Allowlist: `data/ffta/seed_allowlist.json`. Full reference CSVs stay offline under `data/ffta/tables/` —
never import those CSVs at GBA runtime.
