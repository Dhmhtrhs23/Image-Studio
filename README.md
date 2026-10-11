# OpenRouter Image Studio

A GTK4 + Rust desktop client for the [OpenRouter Image API](https://openrouter.ai/docs/guides/overview/multimodal/image-generation).

## Features

- **Model browser** - loads every image model from `GET /api/v1/images/models` and shows input/output modalities,
  streaming support, max reference count, max `n`, the typed `supported_parameters` map, per-provider pricing and
  allowed provider passthrough parameters (from `/images/models/{id}/endpoints`).
- **Image generation** - prompt plus every documented request parameter:
  `n`, `resolution`, `aspect_ratio`, `size`, `quality`, `output_format`, `background`,
  `output_compression`, `seed`, `stream`, `input_references` and `user`.
  Controls are enabled/populated from the selected model capabilities; unsupported controls are disabled.
- **Multiple reference images** - file picker, drag & drop, **clipboard paste (Ctrl+V)**, or public URL / data URI.
  The current model's max-reference count is enforced and shown.
- **Provider routing** - `only`, `order`, `ignore`, `sort`, `allow_fallbacks` and
  `provider.options` JSON passthrough.
- **Streaming** - SSE partial-image preview and final render for models with `supports_streaming`.
- **Results** - gallery with open-in-viewer, save-one, save-all and copy-to-clipboard.
- **Image understanding** - a second tab that sends images to any vision model via
  `/api/v1/chat/completions` with streaming responses.
- **Request JSON preview / log** - inspect exactly what will be sent, and see the stream log.
- API key management with `/key` validation and remaining-credit display (stored `0600` in the config dir;
  `OPENROUTER_API_KEY` overrides it).

## Build from source

Requirements: Rust (stable), GTK 4.10+, `libgtk-4-dev`, `pkg-config`, `gcc`.

```sh
cargo build --release
./target/release/openrouter-image-studio
```

Set your key:

```sh
export OPENROUTER_API_KEY=sk-or-v1-...
```

or click the key icon in the header bar and paste it (saved to
`~/.config/openrouter-image-studio/config.json`).

## Build the AppImage

```sh
bash packaging/build_appimage.sh
```

The result is `openrouter-image-studio-x86_64.AppImage`. If FUSE is unavailable:

```sh
./openrouter-image-studio-x86_64.AppImage --appimage-extract-and-run
```

## Headless probes (for automation)

```sh
# list image models
openrouter-image-studio --list-models

# generate an image
openrouter-image-studio --generate meta/muse-image "a red apple" --out /tmp/out

# generate with reference images and streaming
openrouter-image-studio --generate openai/gpt-image-1-mini "a red cube" --stream --refs a.png,b.png --n 2
```

## Keyboard shortcuts

| Shortcut | Action |
| - | - |
| Ctrl+Enter | Generate (while the prompt is focused) |
| Ctrl+V | Paste a reference image from the clipboard |
| Ctrl+R | Reload the model list |
