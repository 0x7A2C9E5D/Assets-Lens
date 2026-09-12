const enUS = {
    app: {
        title: 'Assets Lens',
        subtitle: "Baldur's Gate 3 · Visual Assets",
    },

    window: {
        minimize: 'Minimize',
        maximize: 'Maximize',
        restore: 'Restore',
        close: 'Close',
    },

    nav: {
        database: 'Database',
        databaseHint: 'Choose a game data folder and scan it',
        browse: 'Browse',
        browseHint: 'Browse the scanned game assets',
        about: 'About',
        aboutHint: 'Version info',
    },

    about: {
        title: 'About',
        subtitle: 'Version info',
        introTitle: 'About this app',
        intro:
            'Assets Lens is a local desktop tool: it scans your Baldur\u2019s Gate 3 game data, indexes the visual assets (meshes, materials, textures and virtual textures), and offers 3D previews and one-click export.',
        stackTitle: 'Credits',
        stack: {
            desktop: 'Desktop framework',
            ui: 'Interface & interaction',
            parsing: 'Game resource parsing',
            preview: '3D model rendering',
        },
        github: 'GitHub repository',
        nexusmods: 'Nexus Mods',
        checking: 'Checking…',
        updateAvailable: 'Version {version} is available on Nexus Mods',
        rightsTitle: 'Rights statement',
        disclaimer:
            'This tool is an independently developed, unofficial third-party application, neither affiliated with nor endorsed by Larian Studios. All trademarks and copyrights related to the game and its assets belong to their respective owners.',
        privacyTitle: 'Privacy promise',
        privacy:
            'We fully protect your privacy and do not collect any of your data; all content is processed offline, solely on your device.',
        licenseTitle: 'Usage restriction',
        license:
            'This tool is provided for personal, non-commercial use only. Any commercial use is prohibited.',
    },

    language: {
        label: 'Language',
    },

    database: {
        title: 'Database',
        subtitle: 'Choose your game data folder and scan it to build the asset database',
        detectFailed: 'No game location detected automatically; please choose the folder manually',
        status: {
            building: 'Scanning',
            ready: 'Ready',
            pending: 'Not scanned',
            unset: 'No folder selected',
        },
        dir: {
            label: 'Game data folder',
            empty: 'No folder selected yet',
            detect: 'Auto Detect',
            choose: 'Choose Folder',
            tip: 'Try Auto Detect first; if that misses, choose the game resources folder.',
        },
        build: {
            title: 'Scan Game Assets',
            desc: 'The first scan reads the game files and records visual assets; it may take a few seconds',
            rebuildTitle: 'Rescan Game Assets',
            rebuildDesc: 'Database is ready; scan again to sync the latest game assets',
            elapsed: 'Elapsed {s}s',
            actionBuilding: 'Scanning…',
            actionRebuild: 'Scan Again',
            actionBuild: 'Scan',
        },
    },

    stats: {
        visualHint: 'Visual assets',
        materialHint: 'Materials',
        textureHint: 'Textures',
        virtualHint: 'Virtual textures',
    },

    browse: {
        title: 'Browse Assets',
        subtitle: 'Sort by name or ID from the column headers',
        searchPlaceholder: 'Search by name or ID',
        searchClearAria: 'Clear search',
        searchEmpty: 'No assets match "{term}"',
        searchClearAction: 'Clear search',
        emptyTitle: 'No assets scanned yet',
        emptyDescription: 'Go to Database, choose the game data folder and scan it.',
        emptyAction: 'Go to Database',
    },

    table: {
        headerName: 'Asset Name',
        headerUuid: 'ID',
        headerMaterial: 'Materials',
        headerTexture: 'Textures',
        headerVirtual: 'Virtual Textures',
        sortByName: 'Sort by name',
        sortById: 'Sort by ID',
        empty: 'No entries to display',
    },

    pagination: {
        range: '{from}-{to} of {total}',
        rangeEmpty: '0 entries',
        first: 'First page',
        prev: 'Previous page',
        next: 'Next page',
        last: 'Last page',
    },

    progress: {
        working: 'Scanning assets…',
    },

    detail: {
        loading: 'Loading details…',
        placeholder: 'Select an entry on the left to view it',
        visualAsset: 'Visual Asset',
        mesh: 'Mesh file',
        pakLabel: 'PAK archive',
        materialIds: 'Materials · {count}',
        materialLabel: 'Materials',
        none: 'None',
        textures: 'Textures · {count}',
        textureLabel: 'Textures',
        noTextures: 'This asset has no textures',
        virtualTextures: 'Virtual Textures · {count}',
        virtualTextureLabel: 'Virtual Textures',
        noVirtualTextures: 'This asset has no virtual textures',
    },

    preview: {
        title: '3D Preview',
        loading: 'Converting model…',
        noMesh: 'This asset has no mesh to preview',
        failed: 'Could not build the model preview',
    },

    export: {
        action: 'Export',
        title: 'Export Asset',
        emptyName: 'No asset selected',
        destLabel: 'Export to',
        destEmpty: 'No folder selected yet',
        pickDir: 'Choose Folder',
        meshFormatLabel: 'Mesh format',
        meshFormat: {
            gr2: 'Raw GR2',
            gr2Desc: 'Export the raw mesh from the archive, without conversion',
            glb: 'Converted GLB',
            glbDesc: 'Convert the mesh to a portable GLB; textures are exported separately',
        },
        textureGroup: 'Textures',
        textureFormat: {
            none: 'None',
            noneDesc: 'Do not export texture files',
            dds: 'DDS',
            ddsDesc: 'Export DDS textures (including virtual textures)',
            png: 'PNG',
            pngDesc: 'Export PNG and discard DDS',
        },
        start: 'Start Export',
        running: 'Exporting…',
        close: 'Close',
        again: 'Export Again',
        phase: {
            prepare: 'Preparing…',
            model: 'Converting model…',
            modelRaw: 'Exporting raw mesh…',
            textures: 'Exporting textures…',
            virtualTextures: 'Extracting virtual textures…',
            manifest: 'Writing manifest…',
            done: 'Export complete',
        },
        resultTitle: 'Exported {count} files',
        filesTitle: 'Files · {count}',
        warningsTitle: 'Notes · {count}',
        warning: {
            textureUnavailable: 'Texture unavailable: {detail}',
            textureWriteFailed: 'Failed to write texture: {detail}',
            pngConvertFailed: 'PNG conversion failed; kept DDS: {detail}',
            pngWriteFailed: 'Failed to write PNG: {detail}',
            vtGtpNotFound: 'No GTP file found for GTex hash "{detail}"',
            vtFailed: 'Virtual texture extraction failed: {detail}',
            manifestWriteFailed: 'Failed to write asset.json: {detail}',
            manifestSerializeFailed: 'Failed to serialize asset.json: {detail}',
        },
    },

    errors: {
        loadList: 'Could not load the asset list. Please try again.',
        loadDetail: 'Could not load the asset details. Please try again.',
        buildFailed: 'The asset scan was not completed. Please try again.',
        directoryFailed: 'Could not apply the chosen folder. Please try again.',
        general: 'Something went wrong. Please try again.',
    },
}

/** The message contract, owned here because the fallback language is the always-complete one */
export type LocaleMessages = typeof enUS

export default enUS
