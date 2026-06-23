# Upstream References

The reference repositories are cloned locally for implementation research only.
They are intentionally ignored by git so this project does not vendor upstream
source trees.

## Pinned Repositories

| Name | Repository | Pinned SHA | Role |
| --- | --- | --- | --- |
| ccx | https://github.com/BenedictKing/ccx | `9128146f8141418e19ccf40ada00e20946fd713f` | Local API proxy, channel routing, admin API, and protocol behavior reference. |
| cc-switch | https://github.com/farion1231/cc-switch | `c67494bafc3582ea3543aaa4626401db157e04b4` | Tauri 2, SQLite, provider switching, local config writes, tray, and macOS app reference. |

## Refresh Commands

```sh
git -C references/ccx fetch origin
git -C references/ccx checkout 9128146f8141418e19ccf40ada00e20946fd713f

git -C references/cc-switch fetch origin
git -C references/cc-switch checkout c67494bafc3582ea3543aaa4626401db157e04b4
```

To update either reference, change the pinned SHA above after reviewing the
upstream diff and keeping the clone directories ignored.
