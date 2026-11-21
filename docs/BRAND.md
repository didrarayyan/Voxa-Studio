# Voxa Studio Brand Guidelines

Official identity, logo usage, color tokens, and typography reference for **Voxa Studio**.

---

## Brand Positioning

| Element | Specification |
| :--- | :--- |
| **Product Name** | **Voxa Studio** |
| **Header Lockup** | **Voxa** + **`STUDIO`** pill badge |
| **Executable** | `Voxa-Studio.exe` |
| **Category** | Local-First Neural Voice, Expressive Narration & Zero-Shot Cloning Studio |
| **Personality** | Precision, dark-studio calm, tactile acoustic clarity, approachable power |

---

## Logo System

The **Voxa Acoustic V-Crest** emblem combines a rounded obsidian squircle container, converging twin geometric `V` wings, and three centered acoustic equalizer bars representing neural synthesis energy.

| Asset File | Purpose | Dimensions / ViewBox |
| :--- | :--- | :--- |
| [`assets/logo-mark.svg`](../assets/logo-mark.svg) | Master vector icon emblem | `512 × 512` |
| [`assets/logo-wordmark-dark.svg`](../assets/logo-wordmark-dark.svg) | Header lockup for dark backgrounds | `430 × 96` |
| [`assets/logo-wordmark-light.svg`](../assets/logo-wordmark-light.svg) | Header lockup for light backgrounds | `430 × 96` |
| [`assets/voxa-icon-512.png`](../assets/voxa-icon-512.png) | High-DPI transparent PNG icon | `512 × 512` RGBA |
| [`voxa-studio/assets/voxa-icon-256.png`](../voxa-studio/assets/voxa-icon-256.png) | Embedded runtime window icon | `256 × 256` RGBA |
| [`voxa-studio/assets/voxa-icon.ico`](../voxa-studio/assets/voxa-icon.ico) | Multi-res Windows Explorer / taskbar icon | `16` to `256 px` |
| [`assets/hero-banner.png`](../assets/hero-banner.png) | GitHub & release showcase banner | `16:9` widescreen |

### Clear Space & Integrity Rules

- Maintain minimum clear space equal to `0.25×` the icon height around all sides of the mark or horizontal wordmark lockup.
- Never skew, stretch, rotate, or recolor individual wings/equalizer bars outside approved brand tokens.
- Keep the obsidian backing container intact so the electric cyan V-crest preserves contrast on both light and dark surfaces.

---

## Color Palette (`voxa-studio/src/theme.rs`)

### Dark Studio Mode (Default)

| Token | Hex | RGB | Usage |
| :--- | :--- | :--- | :--- |
| `bg` | `#0E1015` | `14, 16, 21` | Primary window & canvas backdrop |
| `panel_bg` | `#151821` | `21, 24, 33` | Docked header, sidebars & timeline bar |
| `card_bg` | `#1C202B` | `28, 32, 43` | Elevated content cards |
| `card_alt` | `#232836` | `35, 40, 54` | Hover & secondary interactive containers |
| `input_bg` | `#11131A` | `17, 19, 26` | Script editor & waveform track floor |
| `border` | `#262C3A` | `38, 44, 58` | 1px structural hairlines |
| `ink` | `#F1F5F9` | `241, 245, 249` | Primary headings & body copy |
| `ink_muted` | `#94A3B8` | `148, 163, 184` | Secondary labels & metadata |
| `primary` | `#00D4E5` | `0, 212, 229` | Primary CTA, active playhead, active border |
| `accent_blue` | `#38BDF8` | `56, 189, 248` | Secondary accent & vocal event tags |

### Light Studio Mode

| Token | Hex | RGB | Usage |
| :--- | :--- | :--- | :--- |
| `bg` | `#F1F5F9` | `241, 245, 249` | Canvas backdrop |
| `panel_bg` / `card_bg` | `#FFFFFF` | `255, 255, 255` | Elevated surfaces & header |
| `border` | `#E2E8F0` | `226, 232, 240` | Structural borders |
| `ink` | `#0F172A` | `15, 23, 42` | High-contrast body text |
| `ink_muted` | `#475569` | `71, 85, 105` | Captions & secondary labels |
| `primary` | `#0891B2` | `8, 145, 178` | Primary cyan-teal action color (WCAG AA compliant) |
| `accent_blue` | `#0284C7` | `2, 132, 199` | Secondary highlights & chips |

### Semantic Expression Chips

| Tag Kind | Dark Hex | Light Hex | Example |
| :--- | :--- | :--- | :--- |
| Emotion Tone | `#A78BFA` | `#7C3AED` | `[joyful]` |
| Vocal Event Cue | `#38BDF8` | `#0284C7` | `<laugh>` |
| Word Emphasis | `#FBBF24` | `#D97706` | `(((amazing)))` |

---

## Typography

- **UI Font Stack**: Segoe UI (`segoeui.ttf`), Nirmala UI, Malgun Gothic, Microsoft YaHei, Yu Gothic (automatically registered in `voxa-studio/src/main.rs`).
- **UI Scale**: Headings `18 px`, Wordmark `17.5 px`, Body `14 px`, Buttons `13.5 px`, Captions/Pills `11–12 px`, Timecodes Monospace `9.5–13 px`.
