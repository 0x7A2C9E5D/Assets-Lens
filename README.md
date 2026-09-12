# Assets Lens · BG3 Asset Explorer

[English](#english) · [中文](#中文)

---

## English

A **Tauri 2** desktop tool: it locates the local *Baldur's Gate 3* Data directory, parses the `_merged` asset database out of `Shared.pak`, and lets you page through the visual assets it contains (GR2 meshes + materials + textures + virtual textures), with a live 3D preview and one-click export.

### Features

- **Database page**: auto-detect the default Steam install path, or pick the Data directory containing `Shared.pak` through the native folder dialog; while building the merged index, per-file progress (pushed over a `Channel`) and elapsed time are shown, followed by Visual / Material / Texture / Virtual Texture stat cards
- **Browse page**: a stably sorted, paginated list of visual assets with debounced keyword search and `↑` / `↓` keyboard navigation; selecting an entry shows its 3D preview, GR2 mesh path, source PAK, material IDs, DDS texture list and virtual texture hashes in the right-hand detail panel
- **3D preview**: an embedded three.js viewport loaded on demand; the backend converts GR2 to GLB and ships it as Base64 — geometry only, no textures, rendered with a neutral unlit material so broken normals or missing maps can never turn the model black
- **Asset export**: the detail panel's *Export* button opens format options and a target directory, then writes to `<target>/<asset name>/`; progress is pushed phase by phase and a single missing item only records a note instead of aborting the export
- **Localized UI**: bundles for `en-US` / `zh-CN` / `zh-TW`; the first launch follows the system language (`Intl` + CLDR matching rules) and a manual choice is remembered permanently
- **Frameless window**: a custom title bar with window controls; blank areas of the title bar and sidebar drag the window, double-click maximizes

### Tech stack

| Layer | Technology |
| --- | --- |
| Desktop shell | Tauri 2 (the `assets_lens_lib` static-library layout) |
| Backend | Rust + [maclarian](https://crates.io/crates/maclarian) 0.1.3 (`default-features = false`, i.e. without its CLI feature) + `base64` |
| Frontend | Vite 5 + Vue 3 + TypeScript + Tailwind CSS 3.4 + vue-router 4 + vue-i18n 11 + lucide-vue-next |
| 3D | three.js 0.185 (`GLTFLoader.parse` for the GLB, `OrbitControls` for interaction, lazy-loaded via dynamic `import()`) |
| IPC | `#[tauri::command]` + `invoke`; build/export progress over `tauri::ipc::Channel` |

### Project structure

```
tauri-app/
├─ src/                        Frontend (Vue 3 + TypeScript)
│  ├─ main.ts                  Entry: mounts Vue, registers router and i18n
│  ├─ App.vue                  Shell layout: sidebar + title bar + RouterView
│  ├─ router/index.ts          Routes (hash mode, views lazy-loaded)
│  ├─ views/
│  │  ├─ DatabaseView.vue      Database: folder pick / auto-detect / build / stats
│  │  ├─ BrowseView.vue        Browse: search + pagination + list + detail
│  │  └─ AboutView.vue         About: version, credits, rights and privacy
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
│  ├─ i18n/
│  │  ├─ index.ts              vue-i18n setup, system-locale matching, persistence
│  │  └─ locales/              en-US.ts / zh-CN.ts / zh-TW.ts
│  └─ utils/settings.ts        localStorage persistence (game path, locale)
└─ src-tauri/                  Backend (Rust)
   ├─ src/main.rs              Binary entry point
   ├─ src/lib.rs               Tauri builder: plugins, AppState, command list
   ├─ src/models.rs            serde DTOs (uniform `camelCase` JSON contract)
   ├─ src/state.rs             AppState: resolver / database / sort cache / PAK pool / GTP index
   ├─ src/commands.rs          `#[tauri::command]` implementations
   ├─ src/export.rs            Export pipeline + `Package` (PAK pool) + GTP/GTS handling
   ├─ tauri.conf.json          Window and bundling configuration
   └─ Cargo.toml               Dependency manifest
```

### Architecture

**Backend commands** (`src-tauri/src/commands.rs`, registered in `lib.rs`)

| Command | Description |
| --- | --- |
| `app_info` | Application version (compile-time constant) |
| `detect_game_path` | Auto-detect the BG3 Data directory (default Steam location) |
| `get_game_path` / `set_game_path` | Read / set the Data directory (validates that `Shared.pak` exists) |
| `build_database` | Parses `Shared.pak` into the `_merged` index, streaming progress via `Channel<BuildProgress>` |
| `db_stats` | Current database statistics (`null` before a build) |
| `list_visuals` | Paged visual asset summaries (optional keyword filter) |
| `get_visual` | Detail of one visual asset (materials / textures / virtual textures) |
| `get_visual_preview` | Reads a GR2 and converts it to GLB, returned as Base64 for the 3D preview |
| `export_visual_asset` | Exports one asset, streaming phase progress via `Channel<ExportProgress>` |

**State and caches** (`state.rs`)

- The game directory is **never persisted on disk**: the frontend stores it in localStorage and hands it back through `set_game_path` on startup, so backend state only lives for the current session
- `visual_names()` / `gr2_files()` come from HashMap iteration and are not order-stable; after a build they are sorted and cached in `AppState`, and paging only slices — otherwise pages would shuffle
- `packages` (a `Package`) is a PAK read pool shared across commands: opening an archive parses its whole file table, so it is created once per game directory, capped by `MAX_CACHED_PAKS = 6`
- `texture_index` is a global index of `.gtp` paths, built lazily, used to resolve GTex hashes during virtual texture export

**Export artifacts** (`export.rs`)

```
<target>/<asset name>/
├─ <asset name>.gr2 | <asset name>.glb   Mesh (raw GR2 or converted GLB)
├─ textures/<name>.dds | .png            Referenced DDS textures (optionally PNG)
├─ virtual_textures/<name>_Base.*        Three virtual texture layers
│                  <name>_Normal.*       (Base / Normal / Physical)
│                  <name>_Physical.*
└─ asset.json                            Manifest (maclarian version, materials, textures, files)
```

- Every conversion reuses maclarian (`convert_gr2_bytes_to_glb`, `dds_bytes_to_png_bytes`, `VirtualTextureExtractor`, `LspkReader`) instead of reimplementing anything
- The mesh is the core artifact — its failure aborts the export; a single texture / virtual texture failure only records an `ExportWarning` (`code` is localized on the frontend, `detail` keeps the raw message)
- Virtual textures are staged to a temp directory as `GTP` / `GTS`, extracted, and the temp directory is cleaned up afterwards

### Development

Prerequisites: Rust toolchain + Tauri 2 CLI + Node.js (Vite 5 needs Node 18+).

```bash
npm install          # Install frontend dependencies
npm run dev          # Frontend only (Vite :5173)
npm run build        # Build output into dist/

cargo tauri dev      # Run the desktop app end to end
cargo tauri build    # Bundle (NSIS target on Windows)
```

> `beforeDevCommand` / `beforeBuildCommand` in `tauri.conf.json` use `npm run ...`; if your environment uses bun, change them accordingly.

### Design notes

- **The IPC contract only depends on field names**: the backend emits `#[serde(rename_all = "camelCase")]`, so type names need not match across the boundary (e.g. Rust's `VisualAssetDetail` ↔ TS's `VisualAsset`)
- **GLB is Base64, not `Vec<u8>`**: a byte vector serializes through serde as one JSON number per byte, inflating a multi-MB model to tens of MB; Base64 grows by only ~33% and keeps everything in memory with no temp files
- **Previews and exports run inside `spawn_blocking`**: GR2 decompression/BitKnit decoding and PAK reads take seconds; this keeps the UI responsive and avoids holding the state lock for long
- **Path normalization**: maclarian compares archive entries with `==` on the raw path, which never matches on Windows (`\` vs `/`), so `Package` normalizes separators and casing before comparing
- **`<Name>_<n>.pak` data partitions are excluded**: they carry no LSPK header of their own, cannot be opened standalone, and are reachable through their main archive
- **WebView2 compatibility**: `RouterView` is not wrapped in `<Transition>` (an `out-in` transition gets stuck between leave/enter in WebView2 and renders a blank screen); it renders directly with a bound `:key`
- **three.js context release**: besides `dispose()`, unmounting calls `forceContextLoss()`; otherwise WebView2 discards the oldest context after ~16, which shows up as a black preview

### License

This project depends on `maclarian`, which is licensed under the **PolyForm Noncommercial License 1.0.0**, so this tool is **for non-commercial use only**.

This tool is an independently developed, unofficial third-party application, neither affiliated with nor endorsed by Larian Studios. All trademarks and copyrights related to the game and its assets belong to their respective owners.

---

## 中文

基于 **Tauri 2** 的桌面工具：定位本地《Baldur's Gate 3》的 Data 目录，从 `Shared.pak` 解析 `_merged` 资源数据库，分页浏览其中的视觉资源（GR2 网格 + 材质 + 纹理 + 虚拟纹理），提供实时 3D 预览与一键导出。

### 功能

- **数据库页**：自动检测 Steam 默认安装路径，或用系统原生目录对话框手动选择含 `Shared.pak` 的 Data 目录；构建合并索引时通过 `Channel` 推送逐文件进度并显示耗时，完成后展示 Visual / Material / Texture / Virtual Texture 统计卡片
- **浏览页**：按名称对视觉资源稳定排序分页浏览，支持关键字搜索（防抖过滤）与键盘 `↑` / `↓` 依次切换；点击条目在右侧详情面板查看 3D 预览、GR2 网格路径、来源 PAK、材质 ID、DDS 纹理列表与虚拟纹理哈希
- **3D 预览**：详情面板内嵌 three.js 视口，按需加载；后端把 GR2 转换成 GLB 后以 Base64 传给前端，纯几何、无贴图，使用中性灰无光照材质，避免法线/贴图缺失导致模型全黑
- **资源导出**：详情面板「导出」按钮 → 选择网格格式与纹理格式、目标目录，导出到 `<目标目录>/<资源名>/`；逐阶段推送进度，单项缺失只记「提示」不中断整个导出
- **多语言界面**：内置 `en-US` / `zh-CN` / `zh-TW` 三套语言包，首次启动按系统语言（`Intl` + CLDR 匹配规则）自动选择，手动切换后永久记住
- **无边框窗口**：自绘标题栏与窗口控制按钮，标题栏/侧边栏空白处可拖拽、双击最大化

### 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面壳 | Tauri 2（`assets_lens_lib` 静态库结构） |
| 后端 | Rust + [maclarian](https://crates.io/crates/maclarian) 0.1.3（`default-features = false`，即不使用其 CLI 特性）+ `base64` |
| 前端 | Vite 5 + Vue 3 + TypeScript + Tailwind CSS 3.4 + vue-router 4 + vue-i18n 11 + lucide-vue-next |
| 3D | three.js 0.185（`GLTFLoader.parse` 解析 GLB，`OrbitControls` 交互，动态 `import()` 懒加载） |
| 通信 | `#[tauri::command]` + `invoke`；构建/导出进度用 `tauri::ipc::Channel` 推送 |

### 项目结构

```
tauri-app/
├─ src/                        前端（Vue 3 + TypeScript）
│  ├─ main.ts                  应用入口：挂载 Vue、注册 router 与 i18n
│  ├─ App.vue                  外壳布局：侧边导航 + 标题栏 + RouterView
│  ├─ router/index.ts          路由（hash 模式，页面按需懒加载）
│  ├─ views/
│  │  ├─ DatabaseView.vue      数据库页：目录选择 / 自动检测 / 构建索引 / 统计
│  │  ├─ BrowseView.vue        浏览页：搜索 + 分页 + 列表 + 详情
│  │  └─ AboutView.vue         关于页：版本、技术栈、权利与隐私声明
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
│  ├─ i18n/
│  │  ├─ index.ts              vue-i18n 实例、系统语言匹配（Intl）、语言持久化
│  │  └─ locales/              en-US.ts / zh-CN.ts / zh-TW.ts
│  └─ utils/settings.ts        localStorage 持久化（游戏目录、语言偏好）
└─ src-tauri/                  后端（Rust）
   ├─ src/main.rs              二进制入口
   ├─ src/lib.rs               Tauri Builder：插件注册、AppState 托管、命令清单
   ├─ src/models.rs            serde DTO（统一 `camelCase` JSON 契约）
   ├─ src/state.rs             AppState：resolver / 合并数据库 / 排序缓存 / PAK 池 / GTP 索引
   ├─ src/commands.rs          `#[tauri::command]` 命令实现
   ├─ src/export.rs            导出流水线 + `Package`（PAK 复用池）+ GTP/GTS 解析
   ├─ tauri.conf.json          窗口与打包配置
   └─ Cargo.toml               依赖清单
```

### 架构说明

**后端命令**（`src-tauri/src/commands.rs`，统一注册于 `lib.rs`）

| 命令 | 说明 |
| --- | --- |
| `app_info` | 返回应用版本（编译期常量） |
| `detect_game_path` | 自动检测 BG3 Data 目录（默认 Steam 安装位置） |
| `get_game_path` / `set_game_path` | 读取 / 手动设置 Data 目录（校验 `Shared.pak` 存在） |
| `build_database` | 解析 `Shared.pak` 构建 `_merged` 索引，经 `Channel<BuildProgress>` 推送进度 |
| `db_stats` | 当前数据库统计（未构建时为 `null`） |
| `list_visuals` | 分页（可选关键字过滤）返回视觉资源摘要 |
| `get_visual` | 单个视觉资源详情（材质 / 纹理 / 虚拟纹理） |
| `get_visual_preview` | 读取 GR2 并转换为 GLB，Base64 返回供 3D 预览 |
| `export_visual_asset` | 导出单个资源，经 `Channel<ExportProgress>` 推送阶段进度 |

**状态与缓存**（`state.rs`）

- 游戏目录**不落盘**：前端存在 localStorage，启动时经 `set_game_path` 交回后端校验，后端状态仅存活于当前会话
- `visual_names()` / `gr2_files()` 来自 HashMap 迭代、顺序不稳定，构建后统一排序缓存到 `AppState`，分页只做切片，否则翻页会乱序
- `packages`（`Package`）是跨命令复用的 PAK 读池：打开一个归档要解析整张文件表，因此每个游戏目录只建一次，并设 `MAX_CACHED_PAKS = 6` 上限
- `texture_index` 为 `.gtp` 路径的全局索引（按需延迟构建），用于虚拟纹理导出时的 GTex 哈希查找

**导出产物**（`export.rs`）

```
<目标目录>/<资源名>/
├─ <资源名>.gr2 | <资源名>.glb      网格（原始 GR2 或转换后的 GLB）
├─ textures/<纹理名>.dds | .png     引用的 DDS 纹理（可选转 PNG）
├─ virtual_textures/<名>_Base.*     虚拟纹理三层（Base / Normal / Physical）
│                  <名>_Normal.*
│                  <名>_Physical.*
└─ asset.json                       元数据清单（maclarian 版本、材质、纹理、导出文件列表）
```

- 格式转换全部复用 maclarian（`convert_gr2_bytes_to_glb`、`dds_bytes_to_png_bytes`、`VirtualTextureExtractor`、`LspkReader`），不重复实现
- GLB 网格是核心产物，其失败会中止本次导出；单个纹理 / 虚拟纹理失败只记录 `ExportWarning`（`code` 供前端 i18n 取文案，`detail` 为原始信息）
- 虚拟纹理经临时目录暂存 `GTP` / `GTS` 后提取，结束即清理

### 开发

前置：Rust 工具链 + Tauri 2 CLI + Node.js（Vite 5 需 Node 18+）。

```bash
npm install          # 安装前端依赖
npm run dev          # 仅启动前端（Vite :5173）
npm run build        # 构建产物输出到 dist/

cargo tauri dev      # 端到端运行桌面应用
cargo tauri build    # 打包（Windows 目标为 NSIS）
```

> `tauri.conf.json` 的 `beforeDevCommand` / `beforeBuildCommand` 使用 `npm run ...`；如果你的环境使用 bun，请同步改为对应命令。

### 设计要点

- **IPC 契约只依赖字段名**：后端 `models.rs` 用 `#[serde(rename_all = "camelCase")]` 输出，类型名前后端不必一致（如 Rust 的 `VisualAssetDetail` ↔ TS 的 `VisualAsset`）
- **GLB 用 Base64 而非 `Vec<u8>`**：字节数组经 serde 会序列化成「每字节一个数字」的 JSON，几 MB 模型会膨胀到几十 MB；Base64 只增约 33%，且全程内存操作、无临时文件
- **预览与导出都在 `spawn_blocking` 内执行**：GR2 解压/BitKnit 解码与 PAK 读取耗时数秒，避免阻塞 UI；耗时期间不长时间持有状态锁
- **路径归一化**：maclarian 的归档查找对原始路径做 `==` 比较，Windows 下 `\` 与 `/` 永不匹配，故 `Package` 统一分隔符与大小写后再比对
- **`<Name>_<n>.pak` 数据分片被排除**：它们没有独立 LSPK 头、无法单独打开，且已能通过主归档访问
- **WebView2 兼容**：`RouterView` 不包 `<Transition>`（`out-in` 过渡在 WebView2 上会卡在 leave/enter 之间导致白屏），改为直接渲染并绑定 `:key`
- **three.js 上下文释放**：卸载时除 `dispose()` 外还需 `forceContextLoss()`，否则 WebView2 约 16 个上下文后丢弃最旧的，表现为预览变黑

### 许可

本项目依赖的 `maclarian` 采用 **PolyForm Noncommercial License 1.0.0**，因此本工具 **仅限非商业用途**。

本工具为独立开发的非官方第三方应用，与 Larian Studios 无隶属或背书关系；游戏及其资源相关的商标与版权归各自权利人所有。
