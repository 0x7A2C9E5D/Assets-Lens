# Assets Lens · BG3 Asset Explorer

[English](#english) · [中文](#中文)

---

## English

A **Tauri 2** desktop tool: it locates the local *Baldur's Gate 3* Data directory, parses the `_merged` asset database
out of `Shared.pak`, and lets you page through the visual assets it contains (GR2 meshes + materials + textures +
virtual textures), with a live 3D preview and one-click export.

### Features

- **Database page**: auto-detect the default Steam install directory (Windows / macOS only, and only at the fixed
  default location — a GOG build or a non-default Steam library has to be picked manually), or pick the Data directory
  containing `Shared.pak` through the native folder dialog; while building the merged index, per-file progress (pushed
  over a `Channel`) and elapsed time are shown, followed by Visual / Material / Texture / Virtual Texture stat cards
- **Browse page**: a paginated list of visual assets, sortable by name or GUID from the column headers, with debounced
  keyword search that matches a name or a GUID and `↑` / `↓` keyboard navigation; selecting an entry shows its 3D
  preview, GR2 mesh path and source PAK, material IDs, the DDS texture list with each texture's own source PAK, and, for
  virtual textures, the page file each hash resolved to together with its source archive
- **3D preview**: an embedded three.js viewport loaded on demand; the backend converts GR2 to GLB and ships it as
  Base64 — geometry only, no textures, rendered with a neutral unlit material so broken normals or missing maps can
  never turn the model black
- **Asset export**: the detail panel's *Export* button opens format options and a target directory, then writes to
  `<target>/<asset name>/`; progress is pushed phase by phase and a single missing item only records a note instead of
  aborting the export
- **About page**: the running version sits next to the app name — with a single amber arrow welded into the badge when a
  newer release is out, whose tooltip carries the published number — plus GitHub / Nexus Mods links, the tech stack,
  credits and the rights / privacy / license statements
- **Localized UI**: bundles for `en-US` / `zh-CN` / `zh-TW`; the first launch follows the system language (`Intl` + CLDR
  matching rules) and a manual choice is remembered permanently; switching also updates `<html lang>` and the window
  title
- **Frameless window**: a custom title bar with window controls; blank areas of the title bar and sidebar drag the
  window, double-click maximizes

### Network access

Parsing, previewing and exporting all happen on the local machine, and nothing is ever uploaded. One request does leave
the app: at launch it asks the Nexus Mods GraphQL API which version is published on this mod page, so the About page can
flag an outdated build. The call is anonymous (no account, no API key) and carries nothing but the fixed game / mod ids;
if it fails, only the console hears about it.

### Tech stack

| Layer          | Technology                                                                                                                         |
|----------------|------------------------------------------------------------------------------------------------------------------------------------|
| Desktop shell  | Tauri 2 (the `assets_lens_lib` static-library layout)                                                                              |
| Backend        | Rust + [maclarian](https://crates.io/crates/maclarian) 0.1.3 (`default-features = false`, i.e. without its CLI feature) + `base64` |
| Frontend       | Vite 5 + Vue 3 + TypeScript + Tailwind CSS 3.4 + vue-router 4 + vue-i18n 11 + lucide-vue-next                                      |
| 3D             | three.js 0.185 (`GLTFLoader.parse` for the GLB, `OrbitControls` for interaction, lazy-loaded via dynamic `import()`)               |
| IPC            | `#[tauri::command]` + `invoke`; build/export progress over `tauri::ipc::Channel`                                                   |
| Shell services | `tauri-plugin-dialog` (game directory picker) + `tauri-plugin-opener` (external links open in the system browser)                  |
| Release check  | Nexus Mods GraphQL v2 over `fetch` (anonymous, no API key, no Rust-side proxy)                                                     |

### Project structure

```
tauri-app/
├─ src/                        Frontend (Vue 3 + TypeScript)
│  ├─ main.ts                  Entry: mounts Vue, registers router and i18n, starts the release check
│  ├─ App.vue                  Shell layout: sidebar + title bar + RouterView
│  ├─ index.css                Tailwind entry + shared component classes (glass-card, …)
│  ├─ router/index.ts          Routes (hash mode, views lazy-loaded)
│  ├─ views/
│  │  ├─ DatabaseView.vue      Database: folder pick / auto-detect / build / stats
│  │  ├─ BrowseView.vue        Browse: search + pagination + list + detail
│  │  └─ AboutView.vue         About: version pill + update arrow, links, credits, rights, privacy
│  ├─ components/
│  │  ├─ WindowTitlebar.vue    Frameless custom title bar + window controls
│  │  ├─ LocaleSwitcher.vue    Language switcher
│  │  ├─ AssetTable.vue        Asset list (name + material/texture/virtual counts)
│  │  ├─ AssetDetail.vue       Detail panel (3D preview + lists + export entry)
│  │  ├─ ModelPreview.vue      three.js GLB preview (lazy, cached, GPU-disposed)
│  │  ├─ ExportDialog.vue      Export dialog (options + phase progress + result)
│  │  ├─ PaginationBar.vue     Pagination bar
│  │  ├─ ProgressBar.vue       Progress bar
│  │  ├─ StatCard.vue          Stat card
│  │  └─ EmptyState.vue        Empty state
│  ├─ api/tauri.ts             Frontend IPC: invoke commands + Channel + types
│  ├─ api/nexus.ts             Nexus Mods GraphQL client + dotted-version comparison
│  ├─ i18n/
│  │  ├─ index.ts              vue-i18n setup, system-locale matching, persistence
│  │  └─ locales/              en-US.ts / zh-CN.ts / zh-TW.ts
│  └─ utils/
│     ├─ release.ts            App-wide release state: one update check per launch
│     └─ settings.ts           localStorage persistence (game path, locale)
└─ src-tauri/                  Backend (Rust)
   ├─ src/main.rs              Binary entry point
   ├─ src/lib.rs               Tauri builder: plugins, AppState, command list
   ├─ src/models.rs            serde DTOs (uniform `camelCase` JSON contract)
   ├─ src/state.rs             AppState: resolver / database / sort cache / PAK pool / GTex lookup
   ├─ src/commands.rs          `#[tauri::command]` implementations
   ├─ src/export.rs            Export pipeline (mesh / textures / virtual textures / manifest)
   ├─ src/archives.rs          `Archives`: PAK read pool (file tables, batch archive lookup)
   ├─ src/virtual_textures.rs  GTP/GTS page file lookup and staging for the extractor
   ├─ capabilities/default.json Per-window permissions (dialog, opener, window controls)
   ├─ tauri.conf.json          Window and bundling configuration
   └─ Cargo.toml               Dependency manifest
```

### Architecture

**Backend commands** (`src-tauri/src/commands.rs`, registered in `lib.rs`)

| Command                           | Description                                                                                   |
|-----------------------------------|-----------------------------------------------------------------------------------------------|
| `app_info`                        | Application version (compile-time constant)                                                   |
| `detect_game_path`                | Auto-detect the BG3 Data directory (default Steam location)                                   |
| `get_game_path` / `set_game_path` | Read / set the Data directory (validates that `Shared.pak` exists)                            |
| `build_database`                  | Parses `Shared.pak` into the `_merged` index, streaming progress via `Channel<BuildProgress>` |
| `db_stats`                        | Current database statistics (`null` before a build)                                           |
| `list_visuals`                    | Paged visual asset summaries (optional keyword filter)                                        |
| `get_visual`                      | Detail of one visual asset (materials / textures / virtual textures)                          |
| `get_visual_preview`              | Reads a GR2 and converts it to GLB, returned as Base64 for the 3D preview                     |
| `export_visual_asset`             | Exports one asset, streaming phase progress via `Channel<ExportProgress>`                     |

**State and caches** (`state.rs`)

- The game directory is **never persisted on disk**: the frontend stores it in localStorage and hands it back through
  `set_game_path` on startup, so backend state only lives for the current session
- `visual_names()` / `gr2_files()` come from HashMap iteration and are not order-stable; after a build they are sorted
  and cached in `AppState`, and paging only slices — otherwise pages would shuffle
- `archives` (the `Archives` pool from `archives.rs`) is a PAK read pool shared across commands: opening an archive
  parses its whole file table, so it is created once per game directory, capped by `MAX_CACHED_PAKS = 6`.
  `Archives::locate_many` is what names an archive: one pass over the already-parsed tables, decompressing nothing,
  answers `get_visual` for a mesh and all of its textures at once — a visual easily references a dozen DDS files, and
  rescanning a table of hundreds of thousands of entries per texture would be far too slow
- The pool order is the directory listing with `PAK_PREFERENCE` (`Models.pak` / `Textures.pak`) hoisted to the front,
  and the first archive holding a path wins. Across a full install that rule is unambiguous for everything the app
  labels: 924 of 567,681 entries (0.163%) are listed by two or more archives, but no `.gr2` is, and none of the 11,774
  meshes / 1,717 textures the database references is shared
  (see [Measured on a full install](#measured-on-a-full-install))
- `vt_matches()` resolves a visual's GTex hashes with maclarian's own lookup
  (`MergedResolver::find_gtp_by_hashes_in_pak`), which reports `GtpMatch` values carrying both the `.gtp` path and the
  archive holding it. Only the archive to list is supplied (`VirtualTextures.pak`); a hash that matches nothing renders
  without a page file instead of falling back to a guess. The resolver owns the database it is built from, so the
  database is moved out and back per call

**Release check** (`src/utils/release.ts` + `src/api/nexus.ts`)

- `startReleaseCheck()` runs from `main.ts` before the UI mounts and spends an internal "already checked" flag, so the
  About page — rebuilt on every visit — only reads the result: one request per launch, never per navigation
- The running version reaches the shared state *before* the request is sent (`app_info`, falling back to `package.json`
  when that command is unavailable, e.g. plain-browser dev), so the version pill never waits on the network
- Only two outcomes render anything: `outdated` adds the arrow, `checking` adds a spinner. `latest` and `failed` stay
  silent by design; a failure is logged to the console as `[nexus_release]`
- Versions are compared segment by segment as numbers (`0.10.0` counts as newer than `0.9.0`), with any pre-release /
  build suffix ignored

**Export artifacts** (`export.rs`)

```
<target>/<asset name>/
├─ <asset name>.gr2 | <asset name>.glb   Mesh (raw GR2 or converted GLB)
├─ textures/<name>.dds | .png            Referenced DDS textures (optionally PNG)
├─ virtual_textures/<name>_Albedo.*      Three virtual texture layers
│                  <name>_Normal.*       (Albedo / Normal / Physical)
│                  <name>_Physical.*
└─ asset.json                            Manifest (maclarian version, materials with their
                                         textures)
```

- Every conversion reuses maclarian (`convert_gr2_bytes_to_glb`, `dds_bytes_to_png_bytes`, `VirtualTextureExtractor`,
  `LspkReader`) instead of reimplementing anything
- The mesh is the core artifact — its failure aborts the export; a single texture / virtual texture failure only records
  an `ExportWarning` (`code` is localized on the frontend, `detail` keeps the raw message)
- Virtual textures are staged to a temp directory as `GTP` / `GTS`, extracted, and the temp directory is cleaned up
  afterwards; the page file is read straight out of the archive its match named, and the GTS name is derived from it
  (hash suffix stripped), so the pool-wide scan is left as a fallback only

### Measured on a full install

These numbers come from one complete *Baldur's Gate 3* installation (patch 8, GOG build, Windows) on the **dev**
profile: they show the scale the app is built against rather than a promise, and a different game version shifts them.

| Metric                                 | Value                                                                   |
|----------------------------------------|-------------------------------------------------------------------------|
| `.pak` files under `Data/`             | 54 — 26 main archives, 22 numbered data partitions, 6 localization      |
| File-table entries in the 26 main paks | 567,681                                                                 |
| Database built from `Shared.pak`       | 13,880 visuals, 2,312 materials, 1,904 textures, 1,863 virtual textures |
| Paths listed by two or more archives   | 924 (0.163%) — no `.gr2`, none referenced by the database               |
| `VirtualTextures.pak`                  | 12,974 `.gtp` page files served by 16 `.gts` tile sets                  |
| Naming a mesh and all its textures     | p50 ≈ 1.0 s, p90 ≈ 1.7 s (cold pool)                                    |

### Development

Prerequisites: Rust toolchain + Tauri 2 CLI + Node.js (Vite 5 needs Node 18+).

```bash
npm install          # Install frontend dependencies
npm run dev          # Frontend only (Vite :5173)
npm run build        # Build output into dist/
npx vue-tsc --noEmit # Type-check the frontend without emitting files

cargo tauri dev      # Run the desktop app end to end
cargo tauri build    # Bundle (NSIS target on Windows)
```

> `beforeDevCommand` / `beforeBuildCommand` in `tauri.conf.json` use `npm run ...`; if your environment uses bun, change
> them accordingly.

### Design notes

- **The IPC contract only depends on field names**: the backend emits `#[serde(rename_all = "camelCase")]`, so type
  names need not match across the boundary (e.g. Rust's `VisualAssetDetail` ↔ TS's `VisualAsset`)
- **GLB is Base64, not `Vec<u8>`**: a byte vector serializes through serde as one JSON number per byte, inflating a
  multi-MB model to tens of MB; Base64 grows by only ~33% and keeps everything in memory with no temp files
- **Previews and exports run inside `spawn_blocking`**: GR2 decompression/BitKnit decoding and PAK reads take seconds;
  this keeps the UI responsive and avoids holding the state lock for long
- **Path normalization**: maclarian compares archive entries with `==` on the raw path, which never matches on Windows
  (`\` vs `/`), so `Archives` normalizes separators and casing before comparing
- **`<Name>_<n>.pak` data partitions are excluded**: they carry no LSPK header of their own, cannot be opened
  standalone, and are reachable through their main archive
- **Patch archives delete files with zero-byte entries**: `Patch8_HotFix9.pak` lists almost all of its entries as
  0-byte tombstones. None of them currently lands on a referenced `.gr2` / `.dds`, so "the first archive holding a
  path" still names the right archive; a future patch that tombstones a referenced resource would be reported under
  the earlier archive that still contains it
- **WebView2 compatibility**: `RouterView` is not wrapped in `<Transition>` (an `out-in` transition gets stuck between
  leave/enter in WebView2 and renders a blank screen); it renders directly with a bound `:key`
- **three.js context release**: besides `dispose()`, unmounting calls `forceContextLoss()`; otherwise WebView2 discards
  the oldest context after ~16, which shows up as a black preview
- **The update check belongs to the app, not to a page**: `AboutView` is rebuilt on every route entry, so a page-driven
  check would fire again on each visit; the app spends it once at launch and the page only renders the shared state
- **Nexus Mods is queried straight from the WebView**: the v2 GraphQL endpoint serves mod metadata to anonymous callers
  and answers with `access-control-allow-origin: *`, so no API key, no Rust-side proxy and no extra HTTP dependency in
  the backend are needed
- **The published version is a tooltip, not body text**: the About page shows only the running version, and the
  published number lives in the arrow's `title` / `aria-label` — the row stays quiet while the detail is one hover away
- **One new copy string means three edits**: the locale bundles are held together by `satisfies LocaleMessages`, so
  adding a key to `en-US` without `zh-CN` / `zh-TW` is a compile error
- **A plugin that is not permitted fails at runtime**: command access is granted per window in
  `src-tauri/capabilities/default.json` (currently `dialog:default`, `opener:default` and the window controls), so
  registering a plugin in `lib.rs` alone is not enough

### License

This project depends on `maclarian`, which is licensed under the **PolyForm Noncommercial License 1.0.0**, so this tool
is **for non-commercial use only**.

This tool is an independently developed, unofficial third-party application, neither affiliated with nor endorsed by
Larian Studios. All trademarks and copyrights related to the game and its assets belong to their respective owners.

---

## 中文

基于 **Tauri 2** 的桌面工具：定位本地《Baldur's Gate 3》的 Data 目录，从 `Shared.pak` 解析 `_merged` 资源数据库，分页浏览其中的视觉资源（GR2
网格 + 材质 + 纹理 + 虚拟纹理），提供实时 3D 预览与一键导出。

### 功能

- **数据库页**：自动检测 Steam 默认安装目录（仅 Windows / macOS，且只探测固定默认位置——GOG 版或非默认 Steam
  库需手动选择），或用系统原生目录对话框手动选择含
  `Shared.pak` 的 Data 目录；构建合并索引时通过 `Channel` 推送逐文件进度并显示耗时，完成后展示 Visual / Material /
  Texture / Virtual Texture 统计卡片
- **浏览页**：视觉资源分页浏览，可点击表头按名称或 GUID 排序，并支持按名称或 GUID 的关键字搜索（防抖过滤）与键盘 `↑` / `↓`
  依次切换；点击条目在右侧详情面板查看 3D 预览、GR2 网格路径与来源 PAK、材质 ID、DDS
  纹理列表（每条纹理各自标注来源归档）与虚拟纹理列表（每个哈希解析到的页文件及其来源归档）
- **3D 预览**：详情面板内嵌 three.js 视口，按需加载；后端把 GR2 转换成 GLB 后以 Base64
  传给前端，纯几何、无贴图，使用中性灰无光照材质，避免法线/贴图缺失导致模型全黑
- **资源导出**：详情面板「导出」按钮 → 选择网格格式与纹理格式、目标目录，导出到 `<目标目录>/<资源名>/`
  ；逐阶段推送进度，单项缺失只记「提示」不中断整个导出
- **关于页**：应用名旁显示当前运行版本；当 Nexus Mods 上已发布更新的版本时，版本徽标内会多出一个琥珀色箭头（已发布版本号只出现在悬浮提示里），并提供
  GitHub / Nexus Mods 外链、技术栈、致谢与权利 / 隐私 / 许可声明
- **多语言界面**：内置 `en-US` / `zh-CN` / `zh-TW` 三套语言包，首次启动按系统语言（`Intl` + CLDR 匹配规则）自动选择，手动切换后永久记住；切换同时更新
  `<html lang>` 与窗口标题
- **无边框窗口**：自绘标题栏与窗口控制按钮，标题栏/侧边栏空白处可拖拽、双击最大化

### 网络访问

解析、预览、导出全部在本机完成，不上传任何内容。唯一一次外发请求发生在启动时：向 Nexus Mods GraphQL 查询本 mod
页面已发布的版本，供关于页判断当前是否为旧版本。该请求匿名（无账号、无 API Key），只携带固定的 game / mod
id；失败时只在控制台记录，界面不做任何提示。

### 技术栈

| 层       | 技术                                                                                                                      |
|----------|---------------------------------------------------------------------------------------------------------------------------|
| 桌面壳   | Tauri 2（`assets_lens_lib` 静态库结构）                                                                                   |
| 后端     | Rust + [maclarian](https://crates.io/crates/maclarian) 0.1.3（`default-features = false`，即不使用其 CLI 特性）+ `base64` |
| 前端     | Vite 5 + Vue 3 + TypeScript + Tailwind CSS 3.4 + vue-router 4 + vue-i18n 11 + lucide-vue-next                             |
| 3D       | three.js 0.185（`GLTFLoader.parse` 解析 GLB，`OrbitControls` 交互，动态 `import()` 懒加载）                               |
| 通信     | `#[tauri::command]` + `invoke`；构建/导出进度用 `tauri::ipc::Channel` 推送                                                |
| 系统服务 | `tauri-plugin-dialog`（游戏目录选择）+ `tauri-plugin-opener`（外链交给系统浏览器打开）                                    |
| 版本检查 | Nexus Mods GraphQL v2，前端直接 `fetch`（匿名、无 API Key、无 Rust 侧代理）                                               |

### 项目结构

```
tauri-app/
├─ src/                        前端（Vue 3 + TypeScript）
│  ├─ main.ts                  应用入口：挂载 Vue、注册 router 与 i18n、启动版本检查
│  ├─ App.vue                  外壳布局：侧边导航 + 标题栏 + RouterView
│  ├─ index.css                Tailwind 入口 + 公共组件类（glass-card 等）
│  ├─ router/index.ts          路由（hash 模式，页面按需懒加载）
│  ├─ views/
│  │  ├─ DatabaseView.vue      数据库页：目录选择 / 自动检测 / 构建索引 / 统计
│  │  ├─ BrowseView.vue        浏览页：搜索 + 分页 + 列表 + 详情
│  │  └─ AboutView.vue         关于页：版本徽标 + 更新箭头、外链、技术栈、权利与隐私声明
│  ├─ components/
│  │  ├─ WindowTitlebar.vue    无边框窗口自绘标题栏 + 窗口控制
│  │  ├─ LocaleSwitcher.vue    语言切换
│  │  ├─ AssetTable.vue        资源列表（名称 + 材质/纹理/虚拟纹理计数）
│  │  ├─ AssetDetail.vue       详情面板（3D 预览 + 材质/纹理列表 + 导出入口）
│  │  ├─ ModelPreview.vue      three.js GLB 预览（懒加载、缓存、显存释放）
│  │  ├─ ExportDialog.vue      导出对话框（格式选项 + 分阶段进度 + 结果/提示）
│  │  ├─ PaginationBar.vue     分页栏
│  │  ├─ ProgressBar.vue       进度条
│  │  ├─ StatCard.vue          统计卡片
│  │  └─ EmptyState.vue        空状态
│  ├─ api/tauri.ts             前端 IPC 封装：invoke 命令 + Channel 进度 + 类型定义
│  ├─ api/nexus.ts             Nexus Mods GraphQL 客户端 + 点分版本号比较
│  ├─ i18n/
│  │  ├─ index.ts              vue-i18n 实例、系统语言匹配（Intl）、语言持久化
│  │  └─ locales/              en-US.ts / zh-CN.ts / zh-TW.ts
│  └─ utils/
│     ├─ release.ts            全局版本状态：每次启动只检查一次更新
│     └─ settings.ts           localStorage 持久化（游戏目录、语言偏好）
└─ src-tauri/                  后端（Rust）
   ├─ src/main.rs              二进制入口
   ├─ src/lib.rs               Tauri Builder：插件注册、AppState 托管、命令清单
   ├─ src/models.rs            serde DTO（统一 `camelCase` JSON 契约）
   ├─ src/state.rs             AppState：resolver / 合并数据库 / 排序缓存 / PAK 池 / GTex 查找
   ├─ src/commands.rs          `#[tauri::command]` 命令实现
   ├─ src/export.rs            导出流水线（网格 / 纹理 / 虚拟纹理 / 清单）
   ├─ src/archives.rs          `Archives`：PAK 读取池（文件表、批量归档定位）
   ├─ src/virtual_textures.rs  GTP/GTS 页文件查找与为提取器暂存
   ├─ capabilities/default.json 按窗口授予的权限（dialog、opener、窗口控制）
   ├─ tauri.conf.json          窗口与打包配置
   └─ Cargo.toml               依赖清单
```

### 架构说明

**后端命令**（`src-tauri/src/commands.rs`，统一注册于 `lib.rs`）

| 命令                              | 说明                                                                        |
|-----------------------------------|-----------------------------------------------------------------------------|
| `app_info`                        | 返回应用版本（编译期常量）                                                  |
| `detect_game_path`                | 自动检测 BG3 Data 目录（默认 Steam 安装位置）                               |
| `get_game_path` / `set_game_path` | 读取 / 手动设置 Data 目录（校验 `Shared.pak` 存在）                         |
| `build_database`                  | 解析 `Shared.pak` 构建 `_merged` 索引，经 `Channel<BuildProgress>` 推送进度 |
| `db_stats`                        | 当前数据库统计（未构建时为 `null`）                                         |
| `list_visuals`                    | 分页（可选关键字过滤）返回视觉资源摘要                                      |
| `get_visual`                      | 单个视觉资源详情（材质 / 纹理 / 虚拟纹理）                                  |
| `get_visual_preview`              | 读取 GR2 并转换为 GLB，Base64 返回供 3D 预览                                |
| `export_visual_asset`             | 导出单个资源，经 `Channel<ExportProgress>` 推送阶段进度                     |

**状态与缓存**（`state.rs`）

- 游戏目录 **不落盘**：前端存在 localStorage，启动时经 `set_game_path` 交回后端校验，后端状态仅存活于当前会话
- `visual_names()` / `gr2_files()` 来自 HashMap 迭代、顺序不稳定，构建后统一排序缓存到 `AppState`，分页只做切片，否则翻页会乱序
- `archives`（`archives.rs` 里的 `Archives`）是跨命令复用的 PAK 读池：打开一个归档要解析整张文件表，因此每个游戏目录只建一次，并设
  `MAX_CACHED_PAKS = 6` 上限；给文件标注归档名的是 `Archives::locate_many`——对已解析的表只走一遍、不解压任何文件，一次就回答
  `get_visual` 的「网格 + 全部纹理」；一个视觉资源动辄引用十几张 DDS，若每条纹理都把几十万条的表重扫一遍就太慢了
- 池序 = 目录列出的主档，并把 `PAK_PREFERENCE`（`Models.pak` / `Textures.pak`
  ）提到最前；同一条路径以「第一个含它的归档」为准。完整安装实测下这条规则对本工具的标注范围没有歧义：
  567,681 条表项中有 924 条（0.163%）被两个以上归档列出，但 `.gr2` 零共享，数据库引用的 11774 个网格 / 1717
  张纹理也零共享（见[实测数据](#实测数据)）
- `vt_matches()` 用 maclarian 自带查找（`MergedResolver::find_gtp_by_hashes_in_pak`）解析视觉资源的 GTex 哈希，返回的
  `GtpMatch` 同时携带 `.gtp` 路径与持有它的归档。需要提供的只有「列哪一个归档」（`VirtualTextures.pak`
  ）；匹配不到的哈希不显示页文件，而不是退化成猜测。resolver 持有构建它的那份数据库，因此每次调用都把数据库移出再放回

**版本检查**（`src/utils/release.ts` + `src/api/nexus.ts`）

- `startReleaseCheck()` 由 `main.ts` 在 UI 挂载前调用，并消耗一次「已检查」标记；关于页每次进入都会重建，因此它只读结果：
  **每次启动一次请求，翻页/切换路由不会重发**
- 当前运行版本在请求发出 **之前**就写入共享状态（`app_info`，命令不可用时回退到 `package.json`，例如纯浏览器
  dev），所以版本徽标不会被网络阻塞
- 只有两种状态会渲染内容：`outdated` 增加箭头，`checking` 增加 spinner；`latest` 与 `failed` 一律静默，失败仅在控制台以
  `[nexus_release]` 记录
- 版本号按点分段做数值比较（`0.10.0` 视为新于 `0.9.0`），预发布 / 构建后缀忽略不计

**导出产物**（`export.rs`）

```
<目标目录>/<资源名>/
├─ <资源名>.gr2 | <资源名>.glb      网格（原始 GR2 或转换后的 GLB）
├─ textures/<纹理名>.dds | .png     引用的 DDS 纹理（可选转 PNG）
├─ virtual_textures/<名>_Albedo.*   虚拟纹理三层（Albedo / Normal / Physical）
│                  <名>_Normal.*
│                  <名>_Physical.*
└─ asset.json                       元数据清单（maclarian 版本、材质及其纹理）
```

- 格式转换全部复用 maclarian（`convert_gr2_bytes_to_glb`、`dds_bytes_to_png_bytes`、`VirtualTextureExtractor`、`LspkReader`
  ），不重复实现
- GLB 网格是核心产物，其失败会中止本次导出；单个纹理 / 虚拟纹理失败只记录 `ExportWarning`（`code` 供前端 i18n 取文案，
  `detail` 为原始信息）
- 虚拟纹理经临时目录暂存 `GTP` / `GTS` 后提取，结束即清理；页文件直接从匹配结果指名的归档读取，`GTS` 名由它去掉哈希后缀推导而来，全库扫描只作兜底

### 实测数据

以下数字来自一份完整的《Baldur's Gate 3》安装（补丁 8、GOG 版、Windows）与 **dev** 配置，用来体现本工具面对的数据量级而非承诺值，换一个游戏版本数字即会变化。

| 指标                            | 数值                                                            |
|---------------------------------|-----------------------------------------------------------------|
| `Data/` 下的 `.pak` 文件        | 54 个——26 个主档、22 个编号数据分片、6 个本地化                 |
| 26 个主档的文件表条目           | 567,681 条                                                      |
| 由 `Shared.pak` 构建的数据库    | 13,880 个视觉资源、2,312 个材质、1,904 张纹理、1,863 个虚拟纹理 |
| 被两个以上归档列出的路径        | 924 条（0.163%）——不含 `.gr2`，也不含数据库引用的任何路径       |
| `VirtualTextures.pak`           | 12,974 个 `.gtp` 页文件，由 16 个 `.gts` 瓦片集提供             |
| 标注「网格 + 其全部纹理」的归档 | 冷池 p50 ≈ 1.0 s、p90 ≈ 1.7 s                                   |

### 开发

前置：Rust 工具链 + Tauri 2 CLI + Node.js（Vite 5 需 Node 18+）。

```bash
npm install          # 安装前端依赖
npm run dev          # 仅启动前端（Vite :5173）
npm run build        # 构建产物输出到 dist/
npx vue-tsc --noEmit # 前端类型检查（不产出文件）

cargo tauri dev      # 端到端运行桌面应用
cargo tauri build    # 打包（Windows 目标为 NSIS）
```

> `tauri.conf.json` 的 `beforeDevCommand` / `beforeBuildCommand` 使用 `npm run ...`；如果你的环境使用 bun，请同步改为对应命令。

### 设计要点

- **IPC 契约只依赖字段名**：后端 `models.rs` 用 `#[serde(rename_all = "camelCase")]` 输出，类型名前后端不必一致（如 Rust 的
  `VisualAssetDetail` ↔ TS 的 `VisualAsset`）
- **GLB 用 Base64 而非 `Vec<u8>`**：字节数组经 serde 会序列化成「每字节一个数字」的 JSON，几 MB 模型会膨胀到几十 MB；Base64
  只增约 33%，且全程内存操作、无临时文件
- **预览与导出都在 `spawn_blocking` 内执行**：GR2 解压/BitKnit 解码与 PAK 读取耗时数秒，避免阻塞 UI；耗时期间不长时间持有状态锁
- **路径归一化**：maclarian 的归档查找对原始路径做 `==` 比较，Windows 下 `\` 与 `/` 永不匹配，故 `Archives` 统一分隔符与大小写后再比对
- **`<Name>_<n>.pak` 数据分片被排除**：它们没有独立 LSPK 头、无法单独打开，且已能通过主归档访问
- **补丁归档用 0 字节条目表示「删除该文件」**：`Patch8_HotFix9.pak` 的表项几乎全是 0 字节 tombstone；当前它们都不落在被引用的
  `.gr2` / `.dds` 上，所以「第一个含该路径的归档」仍能报对归档名。若将来某个补丁以 0 字节覆盖被引用的资源，报出的会是仍持有它的那个较早归档
- **WebView2 兼容**：`RouterView` 不包 `<Transition>`（`out-in` 过渡在 WebView2 上会卡在 leave/enter 之间导致白屏），改为直接渲染并绑定
  `:key`
- **three.js 上下文释放**：卸载时除 `dispose()` 外还需 `forceContextLoss()`，否则 WebView2 约 16 个上下文后丢弃最旧的，表现为预览变黑
- **更新检查属于应用而非某个页面**：`AboutView` 每次进入路由都会重建，若由页面驱动检查，来回切换就会重复发请求；因此改为启动时消耗一次，页面只负责渲染共享状态
- **Nexus Mods 由 WebView 直接请求**：v2 GraphQL 端点对匿名调用者开放 mod 元数据，并返回 `access-control-allow-origin: *`
  ，因此无需 API Key、无需 Rust 侧代理，后端也不必引入 HTTP 依赖
- **已发布版本号是提示而非正文**：关于页面上只显示当前运行版本，已发布版本号只存在于箭头的 `title` / `aria-label`
  ；一行保持干净，信息一次悬浮即可见
- **加一条文案 = 改三个文件**：三套语言包由 `satisfies LocaleMessages` 约束，只往 `en-US` 里加键而漏掉 `zh-CN` / `zh-TW`
  会直接编译报错
- **注册了插件不等于有权限**：权限按窗口授予（`src-tauri/capabilities/default.json`，当前为 `dialog:default`、
  `opener:default` 与窗口控制），只在 `lib.rs` 注册插件会在运行时才失败

### 许可

本项目依赖的 `maclarian` 采用 **PolyForm Noncommercial License 1.0.0**，因此本工具 **仅限非商业用途**。

本工具为独立开发的非官方第三方应用，与 Larian Studios 无隶属或背书关系；游戏及其资源相关的商标与版权归各自权利人所有。
