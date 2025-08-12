# 🐾 Tovaras — Desktop Companion Pet

**Tovaras** is a small, animated desktop companion for Linux, written in Rust using [Bevy](https://bevyengine.org/).
It sits on your desktop, wanders around, and keeps you company while you work.
You can make it lazy, playful, or sleepy — it’s up to you.

---

## ✨ Features

- 🖼 **Always on top** — floats above other windows
- 🎨 **Sprite sheet animations** for a cute companion
- 💤 **Idle mode** so it won’t distract you when you’re focused
- 🛠 **Configurable** appearance and behavior
- 🐧 **Linux-first** (X11 & Wayland support via `winit`)

---

## 🚀 Installation

### Prerequisites

- Rust (latest stable)
- Cargo
- Linux desktop environment (AwesomeWM, KDE, GNOME, etc.)

### Clone & Build

```bash
git clone https://github.com/YOUR_USERNAME/tovaras.git
cd tovaras
cargo build --release
```

### Run

```bash
cargo run --release
```

---

## ⚙ AwesomeWM Integration (optional)

To make **Tovaras** act like a sticky, borderless pet that doesn’t appear in your taskbar,
add this to your `rc.lua` in **AwesomeWM**:

```lua
{
  rule_any = { class = { "tovaras", "Tovaras" } },
  properties = {
    border_width = 0,
    titlebars_enabled = false,
    skip_taskbar = true,
    sticky = true,
    ontop = true,
    floating = true,
    focusable = false
  }
}
```

Restart AwesomeWM after saving.

---

## 📦 Assets

Place your sprite sheet in `assets/` and update the animation config in `main.rs` to match your frame size and timing.

---

## 🛠 Development

Run in debug mode:

```bash
cargo run
```

Run with logging:

```bash
RUST_LOG=info cargo run
```

---

## 📜 License

MIT License — see [LICENSE](LICENSE) for details.
