# Source Analyzer

Source Analyzer is a command-line dependency checker for Source Engine BSP maps
and MDL models. It resolves assets against the `SearchPaths` in one
`gameinfo.txt`, including loose files and VPK archives.

The current target is Half-Life 2, Source SDK 2013, and compatible branches such
as Mapbase and Entropy: Zero 2.

## Build

Rust 1.88 or newer is required.

```console
cargo build --release
```

## Usage

```console
source-analyzer map <BSP> [OPTIONS]
source-analyzer model <MDL> [OPTIONS]
```

Options shared by both commands:

- `-p, --profile [NAME]`: load `NAME` from `source-analyzer.toml` next to the
  executable. When omitted, `preset.default` is used.
- `-g, --gameinfo <PATH>`: path to `gameinfo.txt`. Required unless supplied by
  the selected profile.
- `--base-dir <PATH>`: directory used for unprefixed and
  `|all_source_engine_paths|` entries. By default this is inferred as the parent
  of the directory containing `gameinfo.txt`.
- `--missing`: print missing dependencies only.
- `--present`: print present and BSP-embedded dependencies only.
- `--all`: print all dependencies. This is the default.
- `-v, --verbose`: print mount and analysis progress to stderr.

Example:

```console
source-analyzer map maps/example.bsp \
  --gameinfo /path/to/game/gameinfo.txt \
  --all
```

Profile paths are relative to `source-analyzer.toml`. Command-line options take
precedence over profile values.

```toml
[preset]
default = "ez2"

[preset.ez2]
gameinfo = "../EntropyZero2/ez2/gameinfo.txt"
type = "all"
verbose = true
```

With this file next to the executable, `source-analyzer map test.bsp -p` loads
the `ez2` profile. Use `-p ez2` to select it explicitly. `type` accepts `all`,
`missing`, or `present`; profiles may also set `base_dir`.

## Resolution rules

For map analysis, the BSP pakfile has the highest priority. Other resources are
resolved through applicable `game` SearchPaths in their declared order. A
directory entry checks loose files, while a VPK entry checks that archive.
Wildcard entries are expanded in case-insensitive lexical order.

`|gameinfo_path|` is relative to the directory containing `gameinfo.txt`.
`|all_source_engine_paths|` and unprefixed paths are relative to `--base-dir`.
An explicit `name.vpk` entry also resolves Valve's split `name_dir.vpk` form.

Garry's Mod GMA addons, `mount.cfg`, and runtime game mounts are intentionally
outside the scope of the resolver. An ordinary directory explicitly listed in
`SearchPaths` is still handled.

## Analysis scope

Map analysis currently collects:

- world materials from BSP texture data;
- static-prop and entity model references;
- materials used by every model that can be resolved;
- explicit `.vmt`, `.wav`, `.mp3`, and `.pcf` entity values;
- common extensionless entity material and model fields;
- files stored in the BSP pakfile.

Model analysis supports Source MDL versions 27 through 32, 35 through 37, 44
through 49, 52 through 56, and 58 through 59. Gaps are rejected instead of
being interpreted as a similar format. GoldSource formats are outside the
project scope because they do not use Source VMT material resolution. The
analyzer reads only the texture, `cdmaterials`, skin-family, body-part, model,
and mesh fields needed for material resolution. A present material is reported
using the first matching `cdmaterials` candidate. If no candidate exists, every
candidate is reported as missing.

Sound-script names and particle-system names are not expanded to their backing
files yet. Compressed BSP lumps are also not supported yet. These cases fail
explicitly or remain absent from the report rather than being guessed.

## Output

Groups are ordered by status (`MISSING`, `EMBEDDED`, then `PRESENT`) and then by
category (`MAP MATERIAL`, `MAP MODEL`, `MODEL MATERIAL`, `MAP SOUND`, and
`MAP PARTICLE`). Empty groups are omitted.

```text
############################
#  PRESENT MODEL MATERIAL  #
############################

[PRESENT ] [MATERIAL] materials/models/props/barrel.vmt  [vpk: hl2_misc_dir.vpk]
```

The process exits with `0` after a successful analysis, `1` for an input or
analysis error, and clap's standard `2` for invalid command-line arguments.

## Caching

VPK directory indexes are loaded once per process. No persistent cache is used:
the directory data is small enough for typical Source installations, while a
persistent cache would need invalidation and corruption handling. Add one only
if real-world profiling shows startup time to be a problem.
