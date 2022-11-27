# i3bar-river

This is a port of `i3bar` for [river](https://github.com/riverwm/river).

## i3bar compatibility

I've tested [`i3status-rs`](https://github.com/greshake/i3status-rust), [`bumblebee-status`](https://github.com/tobi-wan-kenobi/bumblebee-status) and [`py3status`](https://github.com/ultrabug/py3status) and everything seems usable.

A list of things that are missing (for now):
- `border[_top|_right|_bottom|_left]`
- Click events lack some info (IDK if anyone actually relies on `x`, `y`, `width`, etc.)
- Tray icons

## Advantages

- `river` support (obviously)
- `short_text` switching is "progressive" (see https://github.com/i3/i3/issues/4113)
- Support for rounded corners

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

Example configuration (every parameter is optional):

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
separator_width = 0
tags_r = 6
blocks_r = 6
blocks_overlap = 0
command = "i3status-rs"
```

## How progressive short mode and rounded corners work

Some status bar generators (such as `i3status-rs`) use more than one "json block" per logical block
to implement, for example, buttons.

_Short text management and corner rounding are performed on per-logical-block basis._

`i3bar-river` defines a logical block as a continuous series of "json blocks" with the same `name`.
Also, only the last "json block" is allowed to have a non zero (or absent) `separator_block_width`,
all the other "json blocks" should explicitly set it to zero. If you think this definition is not
the best one, feel free to open a new github issue.

## `blocks_overlap` option

Sometimes `pango` lives a gap between "powerline separators" and the blocks (see https://github.com/greshake/i3status-rust/issues/246#issuecomment-1086753440). In this case, you can set `blocks_overlap` option to number of pixels you want your blocks to overlap. Usually, `1` is a good choice.

## Showcase (with i3status-rs)

### Native separators

![Native separators demo](../assets/native_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
name = "native"
[theme.overrides]
idle_fg = "#ebdbb2"
info_fg = "#458588"
good_fg = "#8ec07c"
warning_fg = "#fabd2f"
critical_fg = "#fb4934"
```

### Powerline separators

![Powerline separators demo](../assets/powerline_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
name = "slick"
```

### Rounded corners

![Rounded corners demo](../assets/rounded_corners_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
separator_width = 0
tags_r = 6
blocks_r = 6
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
name = "slick"
[theme.overrides]
separator = "native"
alternating_tint_bg = "none"
alternating_tint_fg = "none"
```
