# Model Files

Default location: `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models`.

Example download (base English model):

```bash
mise run download-model
```

Pick a size via `SIZE` (English models only):

```bash
SIZE=small mise run download-model
```

Available sizes: `tiny`, `base`, `small`, `medium`, `large`.
