import type {LocaleMessages} from './en-US'

const zhTW = {
    app: {
        title: '資源透鏡',
        subtitle: '柏德之門3 · 視覺資源',
    },

    window: {
        minimize: '最小化',
        maximize: '最大化',
        restore: '還原',
        close: '關閉',
        themeToLight: '切換到淺色主題',
        themeToDark: '切換到深色主題',
    },

    nav: {
        database: '資料庫',
        databaseHint: '選擇遊戲資料資料夾並掃描',
        browse: '瀏覽',
        browseHint: '瀏覽已掃描的遊戲資源',
        about: '關於',
        aboutHint: '版本資訊',
    },

    about: {
        title: '關於',
        subtitle: '版本資訊',
        introTitle: '專案簡介',
        intro:
            '「資源透鏡」是一款本機桌面工具：掃描《柏德之門 3》的遊戲資料，索引其中的視覺資源（網格、材質、紋理與虛擬紋理），並提供 3D 預覽與一鍵匯出。',
        stackTitle: '致謝',
        stack: {
            desktop: '桌面應用程式框架',
            ui: '介面與互動',
            parsing: '遊戲資源解析',
            preview: '3D 模型渲染',
        },
        github: 'GitHub 儲存庫',
        nexusmods: 'Nexus Mods',
        checking: '檢查中…',
        updateAvailable: 'Nexus Mods 上已有新版本 {version}',
        rightsTitle: '權利說明',
        disclaimer:
            '本工具為第三方獨立開發的非官方軟體，與 Larian Studios 無關聯、未獲其背書；遊戲及其資源相關的商標與著作權均歸各自所有者所有。',
        privacyTitle: '隱私承諾',
        privacy:
            '我們充分保障您的隱私安全，不會收集您的任何資料；所有內容均僅在您的裝置上離線處理。',
        licenseTitle: '使用限制',
        license: '本工具僅供個人學習、研究等非商業用途使用，不得用於任何商業用途。',
    },

    language: {
        label: '語言',
    },

    database: {
        title: '資料庫',
        subtitle: '選擇遊戲資料資料夾並掃描，建立視覺化的遊戲資源資料庫',
        detectFailed: '未能自動找到遊戲位置，請手動選擇資源資料夾',
        status: {
            building: '掃描中',
            ready: '已就緒',
            pending: '待掃描',
            unset: '尚未選擇資料夾',
        },
        dir: {
            label: '遊戲資源資料夾',
            empty: '尚未選擇資源資料夾',
            detect: '自動尋找',
            choose: '手動選擇',
            tip: '可先點選「自動尋找」，未找到時再手動選擇遊戲資源資料夾。',
        },
        build: {
            title: '掃描遊戲資源',
            desc: '首次掃描會讀取遊戲檔案並記錄視覺資源，可能需要數秒',
            rebuildTitle: '重新掃描遊戲資源',
            rebuildDesc: '資料庫已就緒，再次掃描即可同步最新的遊戲資源',
            elapsed: '已耗時 {s} 秒',
            actionBuilding: '掃描中…',
            actionRebuild: '重新掃描',
            actionBuild: '開始掃描',
        },
    },

    stats: {
        visualHint: '視覺資源',
        materialHint: '材質',
        textureHint: '紋理',
        virtualHint: '虛擬紋理',
    },

    browse: {
        title: '瀏覽資源',
        subtitle: '點選表頭可依名稱或 ID 排序',
        searchPlaceholder: '依名稱或 ID 搜尋',
        searchClearAria: '清除搜尋',
        searchEmpty: '沒有符合「{term}」的資源',
        searchClearAction: '清除搜尋',
        emptyTitle: '尚未掃描資源',
        emptyDescription: '請先前往「資料庫」，選擇遊戲資料夾並完成掃描。',
        emptyAction: '前往資料庫',
    },

    table: {
        headerName: '資源名稱',
        headerUuid: 'ID',
        headerMaterial: '材質',
        headerTexture: '紋理',
        headerVirtual: '虛擬紋理',
        sortByName: '依名稱排序',
        sortById: '依 ID 排序',
        empty: '沒有可顯示的項目',
    },

    pagination: {
        range: '第 {from}-{to} 筆 / 共 {total} 筆',
        rangeEmpty: '共 0 筆',
        first: '第一頁',
        prev: '上一頁',
        next: '下一頁',
        last: '最後一頁',
    },

    progress: {
        working: '正在掃描資源…',
    },

    detail: {
        loading: '載入詳細資料…',
        placeholder: '在左側選擇項目以查看詳細資料',
        visualAsset: '視覺資源',
        mesh: '網格檔案',
        materialIds: '材質 · {count}',
        materialLabel: '材質',
        none: '無',
        textures: '紋理 · {count}',
        textureLabel: '紋理',
        noTextures: '此資源沒有紋理',
        virtualTextures: '虛擬紋理 · {count}',
        virtualTextureLabel: '虛擬紋理',
        noVirtualTextures: '此資源沒有虛擬紋理',
    },

    preview: {
        title: '3D 預覽',
        loading: '正在轉換模型…',
        noMesh: '此資源沒有可預覽的網格',
        failed: '無法產生模型預覽',
    },

    export: {
        action: '匯出',
        title: '匯出資源',
        emptyName: '未選擇資源',
        destLabel: '匯出至',
        destEmpty: '尚未選擇資料夾',
        pickDir: '選擇資料夾',
        meshFormatLabel: '網格格式',
        meshFormat: {
            gr2: 'GR2 原始格式',
            gr2Desc: '直接匯出封裝內的原始網格，不進行任何轉換',
            glb: 'GLB 轉換格式',
            glbDesc: '轉換為通用 GLB 網格，紋理個別匯出',
        },
        textureGroup: '紋理',
        textureFormat: {
            none: '無',
            noneDesc: '不匯出任何紋理檔案',
            dds: 'DDS',
            ddsDesc: '個別匯出 DDS 紋理（含虛擬紋理）',
            png: 'PNG',
            pngDesc: '匯出 PNG 並捨棄 DDS',
        },
        start: '開始匯出',
        running: '匯出中…',
        close: '關閉',
        again: '再次匯出',
        phase: {
            prepare: '準備中…',
            model: '轉換模型…',
            modelRaw: '匯出原始網格…',
            textures: '匯出紋理…',
            virtualTextures: '擷取虛擬紋理…',
            manifest: '寫入清單…',
            done: '匯出完成',
        },
        resultTitle: '已匯出 {count} 個檔案',
        filesTitle: '匯出檔案 · {count}',
        warningsTitle: '提示 · {count}',
        warning: {
            textureUnavailable: '紋理無法使用：{detail}',
            textureWriteFailed: '紋理寫入失敗：{detail}',
            pngConvertFailed: 'PNG 轉換失敗，已保留 DDS：{detail}',
            pngWriteFailed: 'PNG 寫入失敗：{detail}',
            vtStagingFailed: '虛擬紋理暫存資料夾無法使用：{detail}',
            vtGtpNotFound: '虛擬紋理沒有對應的貼圖檔案，已略過：{detail}',
            vtFailed: '虛擬紋理擷取失敗：{detail}',
            manifestWriteFailed: 'asset.json 寫入失敗：{detail}',
            manifestSerializeFailed: 'asset.json 序列化失敗：{detail}',
        },
    },

    errors: {
        loadList: '資源清單載入失敗，請重試',
        loadDetail: '資源詳細資料載入失敗，請重試',
        buildFailed: '資源掃描未完成，請重試',
        directoryFailed: '資料夾設定未生效，請重試',
        general: '操作未完成，請稍後重試',
    },
} satisfies LocaleMessages

export default zhTW
