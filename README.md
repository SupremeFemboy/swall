# swall

> ⚠️ **100% AI-generated slop:** This code was entirely architected, written, and refactored by AI agents under the supervision of a Supreme Femboy. Use at your own risk.

---

## English

A fast, GPU-accelerated Wayland wallpaper selector written in Rust with `wgpu` and `layershellev`. It opens directly as a native Wayland Layer Shell surface (`zwlr_layer_shell_v1`) on the `Overlay` layer with exclusive keyboard grab. Each wallpaper preview is rendered as a wide 16:9 parallelogram with sides tilted at a 75° angle, complete with sub-pixel edge anti-aliasing, subtle active card zoom, and upright cover texture projection.

Wallpapers are discovered automatically from directories or CLI arguments. An initial starting image path can be piped into `stdin`; upon pressing `Enter`, the canonical path to the selected wallpaper is printed to `stdout` and the program exits immediately. On `Esc`, it closes cleanly with no output. Features a high-speed disk thumbnail cache for instantaneous startups and a customizable TOML configuration with automatic system font resolution via Fontconfig.

### Features

- **Native Wayland Layer Shell.** Binds directly to `zwlr_layer_shell_v1` on `Layer::Overlay`. Sits above all windows, workspaces, and status bars without being managed as an ordinary XDG client window (no window rules or tiling hacks needed).
- **Parallelograms with 75° Slanted Sides (16:9).** Each element has an exact 16:9 aspect ratio ($W = H \times \frac{16}{9}$) with sides slanted at 75° ($\Delta X = H \cdot \cot(75^\circ) \approx 0.268 \times H$).
- **Upright Texture Projection & Anti-Aliasing.** Shaders map wallpaper textures in upright cover mode without geometric shear or distortion, while calculating exact perpendicular edge distances for razor-sharp, anti-aliased borders.
- **`stdin` / `stdout` Integration.** Pipe the currently active wallpaper path into `stdin` to initialize the carousel at that wallpaper; press `Enter` to emit the selected path to `stdout` for chaining with tools like `swww`, `hyprpaper`, or `swaybg`.
- **Lightning-Fast Disk Thumbnail Cache.** Previews are downscaled to 960×540 and cached in binary format under `$XDG_CACHE_HOME/swall/thumbnails/`. Cached loads take less than 0.5 ms per image. Cache keys are computed from file path, size, and modification timestamp (`mtime`), guaranteeing zero stale previews if wallpapers are edited.
- **Fluid Keyboard & Mouse Navigation.** Full support for Left/Right arrows, Vim motions (`h`/`l`), mouse scroll wheel, and direct mouse click selection (clicking the focused card confirms selection). Smooth exponential spring animations on scroll.
- **TOML Configuration.** Customize colors (in `#RRGGBB`, `#RRGGBBAA`, or `[r, g, b, a]` formats), border widths, card opacity, panel toggles, font size, and thumbnail dimensions in `~/.config/swall/config.toml`.
- **System Font Resolution via Fontconfig.** Set `font` to any family name (e.g. `"DepartureMono Nerd Font Mono"`, `"JetBrains Mono"`, `"Noto Sans:style=Bold"`, `"monospace"`) or a direct file path; resolved automatically via `fc-match` and rasterized with sub-pixel anti-aliasing via `ab_glyph`. Seamlessly falls back to a built-in bitmap font if needed.

### Architecture

```
src/
├── main.rs          # layer-shell event loop (layershellev), input dispatch, animation loop
├── config.rs        # TOML config loader, flexible Color deserializer, default template
├── discovery.rs     # wallpaper discovery, non-blocking stdin parser, natural sorting
├── loader.rs        # async thumbnail loader, mtime-keyed disk cache
├── render.rs        # wgpu pipelines, WGSL shaders (parallelogram, vignette, UI quad)
└── ui.rs            # fontconfig resolver (fc-match), ab_glyph/font8x8 text renderer
```

1. **Windowing & Event Loop:** Uses `layershellev::WindowState` configured with `.with_use_display_handle(true)`, `.with_layer(Layer::Overlay)`, `.with_anchor(Top | Bottom | Left | Right)`, and `.with_keyboard_interacivity(KeyboardInteractivity::Exclusive)`.
2. **GPU Rendering:** Passes `layershellev::WindowWrapper` to `wgpu::Instance::create_surface` over Vulkan (`Backends::PRIMARY`). Three rendering passes: background vignette, instanced card quad mesh, and UI text overlay.
3. **Threading:** Image decoding and disk cache generation run asynchronously in a background worker thread, streaming thumbnails over channels to keep the UI running at monitor refresh rate.

### Configuration

Paths adhere to the XDG Base Directory specification:

| File | Path |
|---|---|
| `config.toml` | `$XDG_CONFIG_HOME/swall/config.toml` (default `~/.config/swall/config.toml`) |
| `thumbnails/` | `$XDG_CACHE_HOME/swall/thumbnails/` (default `~/.cache/swall/thumbnails/`) |

If `config.toml` does not exist on startup, it is automatically created with default settings:

```toml
# swall configuration file

[ui]
# Show the top panel with image counter and filename
show_top_bar = true

# Show the bottom panel with keyboard controls
show_bottom_bar = true

# Font name/family (e.g. "DepartureMono Nerd Font Mono", "JetBrains Mono", "monospace")
# or direct file path to a .ttf/.otf font file.
# If empty or not found, built-in crisp bitmap font is used.
font = ""

# Font size in pixels
font_size = 20.0

[colors]
# Colors can be specified as hex string ("#0b0e14", "#0b0e14e1") or array ([r, g, b, a])
background_color = "#0b0e14e1"
panel_bg_color = "#10141adc"
panel_border_color = "#00c8ffb4"

card_border_color = "#ffffff40"
selected_border_color = "#00e1ffff"

card_border_width = 1.5
selected_border_width = 4.5
unselected_opacity = 0.65

text_color = "#f5f8ffff"
accent_color = "#00e1ffff"
dim_text_color = "#8c9baff0"

[cache]
# Enable disk caching of downscaled thumbnails for instant loading
enabled = true

# Maximum thumbnail dimension in pixels (960x540 for 16:9 previews)
max_thumbnail_size = 960
```

### Controls & Keybindings

| Key / Input | Action |
|---|---|
| `Left` / `h` | Move selection to previous wallpaper |
| `Right` / `l` | Move selection to next wallpaper |
| `Mouse Wheel` | Scroll left / right through wallpaper ribbon |
| `Left Click` | Focus clicked card (clicking already focused card confirms) |
| `Enter` / `Numpad Enter` | Select active wallpaper, print path to `stdout`, and exit `0` |
| `Esc` | Close selector and exit without printing to `stdout` |

### CLI Usage & Integration Examples

#### With `swww` (Wayland wallpaper daemon):
```bash
SELECTED=$(echo "$CURRENT_WALLPAPER" | swall ~/Pictures/Wallpapers)
if [ -n "$SELECTED" ]; then
    swww img "$SELECTED" --transition-type wipe --transition-fps 144
fi
```

#### With `hyprpaper`:
```bash
SELECTED=$(swall ~/Pictures/Wallpapers)
if [ -n "$SELECTED" ]; then
    hyprctl hyprpaper preload "$SELECTED"
    hyprctl hyprpaper wallpaper ",$SELECTED"
fi
```

#### Starting on the currently active wallpaper:
```bash
echo "/home/user/Pictures/Wallpapers/sunset.jpg" | swall
```

#### Running without arguments:
Scans `WALLPAPERS_DIR`, `~/Pictures/Wallpapers`, `~/Pictures`, or current directory automatically:
```bash
swall
```

### Build & run

```bash
git clone https://github.com/RustBlyat/swall
cd swall
cargo build --release
./target/release/swall ~/Pictures/Wallpapers
```

### Installing from source (Arch Linux)

An AUR-style `PKGBUILD` is included and builds straight from the git repository:

```bash
git clone https://github.com/RustBlyat/swall
cd swall
makepkg -si
```

The package installs the binary to `/usr/bin/swall`. Runtime dependencies are `glibc`, `wayland`, `fontconfig`, and a Vulkan driver.

### Notes & limitations

- Requires a Wayland compositor implementing the `zwlr_layer_shell_v1` protocol (e.g. Hyprland, Sway, River, Wayfire, Labwc). Does not run on X11 or Mutter/GNOME (which intentionally omit layer-shell support).
- Exclusive keyboard interactivity is grabbed while open, preventing background window inputs until selection is confirmed or dismissed with `Esc`.
- Thumbnails are stored uncompressed in `$XDG_CACHE_HOME/swall/thumbnails/` for sub-millisecond loading.

---

## Русский

Быстрый GPU-ускоренный селектор обоев для Wayland, написанный на Rust с использованием `wgpu` и `layershellev`. Открывается как нативный оверлей Wayland Layer Shell (`zwlr_layer_shell_v1`) на слое `Overlay` с монопольным захватом клавиатурного фокуса. Каждое превью обоев отображается в виде широкого параллелограмма с соотношением сторон 16:9 и скосом боковых граней 75°, субпиксельным сглаживанием, подсветкой активной карточки и вертикальной «cover»-проекцией текстуры без искажений.

Обои автоматически обнаруживаются в указанной папке или стандартных директориях. В `stdin` можно передать путь к текущим обоям для мгновенного старта с нужного изображения; при нажатии `Enter` абсолютный путь выбранного изображения выводится в `stdout`, а программа немедленно завершает работу. При нажатии `Esc` программа закрывается без вывода. Поддерживается высокоскоростной дисковый кэш уменьшенных превью и TOML-конфиг с автоматическим поиском системных шрифтов через Fontconfig.

### Возможности

- **Нативный Wayland Layer Shell.** Работает напрямую с протоколом `zwlr_layer_shell_v1` на слое `Layer::Overlay`. Располагается поверх всех окон, рабочих столов и панелей, не отображаясь в списке обычных окон XDG (не требует правил `windowrule` или настройки плавающего режима).
- **Параллелограммы 16:9 со скосом граней 75°.** Элементы имеют соотношение сторон 16:9 ($W = H \times \frac{16}{9}$) со скосом $75^\circ$ ($\Delta X = H \cdot \cot(75^\circ) \approx 0.268 \times H$).
- **Вертикальная проекция текстуры и сглаживание.** Шейдеры проецируют обои в режиме «cover» строго вертикально без геометрических перекосов самого изображения, рассчитывая точное расстояние до скошенных границ для субпиксельного антиалиасинга.
- **Интеграция со `stdin` и `stdout`.** Передавайте путь текущих обоев в `stdin` для старта с активного изображения; клавиша `Enter` отправляет выбранный путь в `stdout`, что позволяет легко встраивать селектор в скрипты с `swww`, `hyprpaper` или `swaybg`.
- **Быстрый дисковый кэш превью.** Превью уменьшаются до 960×540 и сохраняются в бинарном формате в `$XDG_CACHE_HOME/swall/thumbnails/`. Загрузка из кэша занимает менее 0.5 мс. Ключ кэша строится на основе пути, размера и времени изменения (`mtime`), что исключает показ устаревших превью при замене файлов.
- **Управление клавиатурой и мышью.** Поддерживаются стрелки влево/вправо, Vim-навигация (`h`/`l`), колесо мыши и выбор кликом (повторный клик на активную карточку подтверждает выбор). Плавная экспоненциальная интерполяция прокрутки.
- **Конфигурация TOML.** Настройка цветов (в форматах `#RRGGBB`, `#RRGGBBAA` или `[r, g, b, a]`), толщины рамок, прозрачности, скрытия панелей, размера шрифта и кэша в `~/.config/swall/config.toml`.
- **Автоматический резолвинг шрифтов через Fontconfig.** В параметре `font` можно указывать как семейство шрифта (`"DepartureMono Nerd Font Mono"`, `"JetBrains Mono"`, `"Noto Sans:style=Bold"`, `"monospace"`), так и прямой путь к `.ttf`/`.otf`; резолвинг выполняется через `fc-match` с качественной отрисовкой через `ab_glyph`. При отсутствии файла используется встроенный растровый шрифт.

### Архитектура

```
src/
├── main.rs          # цикл событий layer-shell (layershellev), обработка ввода, анимация
├── config.rs        # загрузка TOML-конфига, десериализатор Color, шаблон по умолчанию
├── discovery.rs     # поиск обоев, неблокирующий разбор stdin, сортировка
├── loader.rs        # асинхронный загрузчик превью, дисковый кэш по mtime
├── render.rs        # конвейеры wgpu, WGSL-шейдеры (параллелограммы, виньетка, оверлей)
└── ui.rs            # резолвер шрифтов (fc-match), рендерер ab_glyph/font8x8
```

1. **Окна и события:** Использует `layershellev::WindowState` с параметрами `.with_use_display_handle(true)`, `.with_layer(Layer::Overlay)`, `.with_anchor(Top | Bottom | Left | Right)` и `.with_keyboard_interacivity(KeyboardInteractivity::Exclusive)`.
2. **GPU-рендеринг:** Отрисовка в поверхность `wgpu` через Vulkan (`Backends::PRIMARY`). Три прохода: виньетка затемнения фона, полигональная сетка параллелограммов и интерфейсный оверлей.
3. **Многопоточность:** Декодирование файлов и генерация кэша выполняются асинхронно в фоновом потоке с передачей готовых текстур через каналы.

### Конфигурация

Пути соответствуют спецификации XDG Base Directory:

| Файл | Путь |
|---|---|
| `config.toml` | `$XDG_CONFIG_HOME/swall/config.toml` (по умолчанию `~/.config/swall/config.toml`) |
| `thumbnails/` | `$XDG_CACHE_HOME/swall/thumbnails/` (по умолчанию `~/.cache/swall/thumbnails/`) |

При первом запуске файл конфигурации создаётся автоматически:

```toml
# swall configuration file

[ui]
# Показывать верхнюю панель с номером и названием файла
show_top_bar = true

# Показывать нижнюю панель с подсказками управления
show_bottom_bar = true

# Имя гарнитуры (например, "DepartureMono Nerd Font Mono", "JetBrains Mono", "monospace")
# или прямой путь к файлу .ttf/.otf.
# Если пустая строка, используется встроенный растровый шрифт.
font = ""

# Размер шрифта в пикселях
font_size = 20.0

[colors]
# Цвета можно указывать HEX строкой ("#0b0e14", "#0b0e14e1") или массивом ([r, g, b, a])
background_color = "#0b0e14e1"
panel_bg_color = "#10141adc"
panel_border_color = "#00c8ffb4"

card_border_color = "#ffffff40"
selected_border_color = "#00e1ffff"

card_border_width = 1.5
selected_border_width = 4.5
unselected_opacity = 0.65

text_color = "#f5f8ffff"
accent_color = "#00e1ffff"
dim_text_color = "#8c9baff0"

[cache]
# Включить дисковый кэш для моментальной подгрузки
enabled = true

# Максимальный размер превью (960px для 16:9 дает четкую картинку 960x540)
max_thumbnail_size = 960
```

### Управление

| Клавиша / Ввод | Действие |
|---|---|
| `Left` / `h` | Предыдущие обои |
| `Right` / `l` | Следующие обои |
| `Колёсико мыши` | Прокрутка ленты влево / вправо |
| `Клик ЛКМ` | Выбор карточки (клик на уже активную подтверждает выбор) |
| `Enter` / `Numpad Enter` | Выбрать обои, вывести путь в `stdout` и завершить работу |
| `Esc` | Закрыть программу без вывода в `stdout` |

### Примеры интеграции

#### Использование с `swww`:
```bash
SELECTED=$(echo "$CURRENT_WALLPAPER" | swall ~/Pictures/Wallpapers)
if [ -n "$SELECTED" ]; then
    swww img "$SELECTED" --transition-type wipe --transition-fps 144
fi
```

#### Использование с `hyprpaper`:
```bash
SELECTED=$(swall ~/Pictures/Wallpapers)
if [ -n "$SELECTED" ]; then
    hyprctl hyprpaper preload "$SELECTED"
    hyprctl hyprpaper wallpaper ",$SELECTED"
fi
```

#### Запуск с выбором текущих обоев:
```bash
echo "/home/user/Pictures/Wallpapers/sunset.jpg" | swall
```

#### Запуск без аргументов:
Автоматически проверяет `WALLPAPERS_DIR`, `~/Pictures/Wallpapers`, `~/Pictures` или текущую папку:
```bash
swall
```

### Сборка и запуск

```bash
git clone https://github.com/RustBlyat/swall
cd swall
cargo build --release
./target/release/swall ~/Pictures/Wallpapers
```

### Установка из исходников (Arch Linux)

В репозитории подготовлен `PKGBUILD` в стиле AUR:

```bash
git clone https://github.com/RustBlyat/swall
cd swall
makepkg -si
```

Бинарник устанавливается в `/usr/bin/swall`. Для работы требуются `glibc`, `wayland`, `fontconfig` и Vulkan-драйвер.

### Примечания и ограничения

- Требуется композитор Wayland с поддержкой протокола `zwlr_layer_shell_v1` (Hyprland, Sway, River, Wayfire, Labwc). Программа не работает в X11 или Mutter/GNOME (где намеренно отсутствует поддержка layer-shell).
- На время открытия захватывается монопольный фокус клавиатуры.
- Превью кэшируются без сжатия в `$XDG_CACHE_HOME/swall/thumbnails/` для предельной скорости чтения с диска.
