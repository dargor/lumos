# ✨ Lumos – Terminal Brightness Detector ✨

A magical ✨ way to detect whether your terminal is in light or dark mode, because CLI apps deserve good aesthetics too.

## Foreword

I made this so Vim can adjust to current Zed mode.

And when I say _I_ made this, I really mean _Claude Sonnet 4_ and _[Qwen3-30B-A3B-Instruct-2507](https://huggingface.co/Qwen/Qwen3-30B-A3B-Instruct-2507)_ made this.

The rest of this file is mostly Claude's work, edited by myself.

## The Problem

Have you ever written a CLI tool that looks great in your dark terminal, only to have someone complain it's unreadable in their light theme? Or vice versa? Terminal applications often hardcode colors that look good in one theme but terrible in another.

**Lumos** solves this by detecting your terminal's background color and telling you whether it's light or dark, so your CLI apps can adapt their color schemes accordingly.

## Quick Start

```bash
# Run tests, build and install in ~/bin/
$ just install

# Use it
$ lumos

# You can also use the python version (no external dependencies)
$ ./lumos.py
```

## How It Works

Lumos uses the **OSC 11** (Operating System Command 11) escape sequence to query your terminal for its background color:

1. **Query**: Sends `\x1b]11;?\x07` to the terminal
2. **Parse**: Terminal responds with color in formats like `rgb:1234/5678/9abc` or `#123456`
3. **Calculate**: Computes relative luminance using the sRGB formula from WCAG guidelines
4. **Decide**: Outputs "dark" if luminance < 0.5, "light" if ≥ 0.5, or "unknown" if detection fails

The luminance calculation follows the standard formula:

```python
L = 0.2126 × R + 0.7152 × G + 0.0722 × B
```

Where `R`, `G` and `B` are linearized RGB values accounting for human vision perception.

## The Journey: From Python Prototype to Rust

### Why Python First?

The initial implementation was written in Python as a quick proof-of-concept.

Python's excellent libraries and rapid development cycle made it perfect for:

- Experimenting with different terminal query approaches
- Testing various color parsing formats
- Validating the luminance calculation
- Ensuring compatibility across different terminals

The Python version (`lumos.py`) clocks in less than 100 LOC, and worked great for exploration.

### Why Rust for Production?

After validating the concept, I rewrote it in Rust for several compelling reasons:

**Performance** 📈

```bash
$ time ./lumos.py
dark./lumos.py  0.03s user 0.01s system 85% cpu 0.043 total

$ time ./target/debug/lumos
dark./target/debug/lumos  0.00s user 0.00s system 87% cpu 0.008 total

$ time ./target/release/lumos
dark./target/release/lumos  0.00s user 0.00s system 84% cpu 0.005 total
```

As you can see, the Rust version is _much_ faster than the Python version.

For a utility that might be called frequently in shell prompts or scripts, this matters.

**Zero Dependencies** 🚀

- Python version: Requires Python runtime + standard library
- Rust version: Single static binary with no runtime dependencies

**Memory Safety** 🛡️

Terminal manipulation involves low-level system calls. Rust's ownership system prevents the memory safety issues that could occur in C/C++ while maintaining performance.

**Better Error Handling** ✨

Rust's `Result` type makes error handling explicit and comprehensive, crucial when dealing with:

- Terminal device access (`/dev/tty`)
- Raw terminal mode manipulation
- Parsing various color formats from different terminals

**Professional Polish** 💎

The Rust version includes:

- Comprehensive documentation with examples
- Full test suite covering edge cases
- Proper error codes and debugging support
- Clippy linting for code quality
- Cargo deny for security auditing

## Supported Terminals

Lumos should work with any terminal that supports OSC 11 queries.

I mostly tested it for my personal use, so your mileage may vary.

Feel free to open a merge request to report other terminals that work or don't work.

### Terminal emulators

| **Terminal emulator** | **Status** |
|-----------------------|------------|
| WezTerm               | ✅         |
| VS Code               | ✅         |
| Zed                   | ✅         |
| Vim                   | ✅         |

### Terminal multiplexers

| **Terminal multiplexer** | **Status** |
|--------------------------|------------|
| screen                   | ✅         |
| tmux                     | ❌         |
| zellij                   | ❌         |

## Color Format Support

Lumos parses multiple color formats returned by different terminals:

```rust
// X11 RGB format (most common)
"rgb:ff00/8000/0000" → (255, 128, 0)

// Hex format
"#ff8000" → (255, 128, 0)

// CSS-style RGB
"rgb(255, 128, 0)" → (255, 128, 0)

// RGBA (alpha ignored)
"rgba:ff00/8000/0000/ffff" → (255, 128, 0)
```

## Integration Examples

### Shell Prompt

```bash
# In your shell scripts or applications
if [ "$(lumos)" = "dark" ]; then
    echo "🌙 Dark mode detected"
else
    echo "☀️ Light mode detected"
fi
```

### Vim

At startup:

```vim
if executable('lumos')
    let g:terminal_background = trim(system('lumos'))
else
    let g:terminal_background = 'unknown'
endif

if g:terminal_background ==# 'light'
    colorscheme catppuccin_latte
else
    colorscheme catppuccin_frappe
endif
```

When gaining focus:

```vim
function! s:lumos()
    let g:terminal_background = trim(system('lumos'))
    if g:terminal_background ==# 'light'
        if g:colors_name !=# 'catppuccin_latte'
            colorscheme catppuccin_latte
        endif
    else
        if g:colors_name !=# 'catppuccin_frappe'
            colorscheme catppuccin_frappe
        endif
    endif
endfunction

autocmd FocusGained * call s:lumos()
```

Note that since `v9.1.1703` Vim has support for OSC 11 queries through its builtin plugin `colorresp.vim`, but for now it prevent colorschemes to load properly (`g:colors_name` undefined -> default colorscheme reported).

I had to disable it like this, on top of my `.vimrc`:

```vim
let g:loaded_colorresp = 1
```

See my [dotfiles](https://github.com/dargor/dotfiles) for more examples.

## Development

```bash
# Quality assurance (security audit, tests...)
$ just qa

# Debug mode (shows detection details)
$ env DEBUG=1 cargo run
```

## Technical Details

The implementation handles several tricky aspects:

**Terminal State Management**: Temporarily switches to raw mode to capture the color response without interfering with normal terminal operation.

**Timeout Handling**: Uses polling with a 2-second timeout to avoid hanging if the terminal doesn't respond.

**Format Flexibility**: Supports the wild variety of color formats that different terminals return.

**Cross-Platform**: Works on Unix-like systems (Linux, macOS, BSD) through direct `/dev/tty` access.

## Exit Codes

- `0`: Successfully detected background (light/dark)
- `2`: Unable to determine background (unknown)

## License

MIT License - see [LICENSE](LICENSE) file.

## Contributing

Contributions welcome! Areas for improvement:

- Windows support (currently Unix-only)
- Additional terminal compatibility
- More color format support
- Integration examples for popular tools

---

_Made with ✨ magic ✨ and a lot of terminal escape sequence debugging._

_No LLM were harmed in the making of this project._
