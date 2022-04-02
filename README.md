# i3bar-river

This is a port of `i3bar` for [river](https://github.com/riverwm/river).

## Warning

It works on my machine™, but this program is in early stage of development.

## i3bar compatibility

Full compatibility is desired, but right now not everything is implemented.

I've tested [`i3status-rs`](https://github.com/greshake/i3status-rust), [`bumblebee-status`](https://github.com/tobi-wan-kenobi/bumblebee-status) and [`py3status`](https://github.com/ultrabug/py3status) and everything seems usable.

A list of things that are missing (for now):
- `short_text`
- `border[_top|_right|_bottom|_left]`
- Click events lack some info (IDK if anyone actually relies on `x`, `y`, `width`, etc.)
- Multiple seat support (river doesn't support this either, so it's fine for now)

## Installation

External dependencies: `libpango1.0-dev`.

Just clone the repo and use `cargo` to build the project:

```
git clone https://github.com/MaxVerevkin/i3bar-river
cd i3bar-river
cargo install --path .
```

Then add this to the end of your river init script:

```
riverctl spawn i3bar-river
```

## Configuration

The configuration file should be stored in `$XDG_CONFIG_HOME/i3bar-river/config.toml` or `~/.config/i3bar-river/config.toml`.

Example configuration (everything except for `command` is optional):

```toml
background = "#282828"
color = "#ffffff"
separator = "#9a8a62"
tag_fg = "#d79921"
tag_bg = "#282828"
tag_focused_fg = "#1d2021"
tag_focused_bg = "#689d68"
tag_urgent_fg = "#282828"
tag_urgent_bg = "#cc241d"
font = "JetBrainsMono Nerd Font 10"
height = 20
command = "~/i3status-rust/target/debug/i3status-rs"
```
