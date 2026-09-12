import type {LocaleMessages} from './en-US'

const zhCN = {
    app: {
        title: '资源透镜',
        subtitle: '博德之门3 · 视觉资源',
    },

    window: {
        minimize: '最小化',
        maximize: '最大化',
        restore: '还原',
        close: '关闭',
    },

    nav: {
        database: '数据库',
        databaseHint: '选择游戏数据目录并扫描',
        browse: '浏览',
        browseHint: '浏览已扫描的游戏资源',
        about: '关于',
        aboutHint: '版本信息与技术细节',
    },

    about: {
        title: '关于',
        subtitle: '应用版本与技术栈',
        introTitle: '项目简介',
        intro:
            '「资源透镜」是一款本地桌面工具：扫描《博德之门 3》的游戏数据，索引其中的视觉资源（网格、材质、纹理与虚拟纹理），并提供 3D 预览与一键导出。',
        stackTitle: '致谢',
        stack: {
            desktop: '桌面应用框架',
            ui: '界面与交互',
            parsing: '游戏资源解析',
            preview: '3D 模型渲染',
        },
        github: 'GitHub 仓库',
        nexusmods: 'Nexus Mods',
        checking: '检查中…',
        updateAvailable: 'Nexus Mods 上已有新版本 {version}',
        rightsTitle: '权利说明',
        disclaimer:
            '本工具为第三方独立开发的非官方软件，与 Larian Studios 无关联、未获其背书；游戏及其资源相关的商标与版权均归各自所有者所有。',
        privacyTitle: '隐私承诺',
        privacy:
            '我们充分保障您的隐私安全，不会收集您的任何数据；所有内容均仅在您的设备上离线处理。',
        licenseTitle: '使用限制',
        license: '本工具仅供个人学习、研究等非商业用途使用，不得用于任何商业用途。',
    },

    language: {
        label: '语言',
    },

    database: {
        title: '数据库',
        subtitle: '选择游戏数据目录并扫描，建立可视化的游戏资源数据库',
        detectFailed: '未能自动找到游戏位置，请手动选择资源目录',
        status: {
            building: '扫描中',
            ready: '已就绪',
            pending: '待扫描',
            unset: '未选择目录',
        },
        dir: {
            label: '游戏资源目录',
            empty: '尚未选择资源目录',
            detect: '自动查找',
            choose: '手动选择',
            tip: '可先点击“自动查找”；若未找到，请手动定位游戏存放资源的文件夹。',
        },
        build: {
            title: '扫描游戏资源',
            desc: '读取游戏文件并登记其中的视觉资源，首次操作可能需要数十秒',
            rebuildTitle: '重新扫描游戏资源',
            rebuildDesc: '数据库已就绪，重新扫描可同步最新游戏资源，通常只需几秒',
            elapsed: '已用时 {s} 秒',
            actionBuilding: '扫描中…',
            actionRebuild: '重新扫描',
            actionBuild: '开始扫描',
        },
    },

    stats: {
        visualHint: '视觉资源',
        materialHint: '材质清单',
        textureHint: '纹理引用',
        virtualHint: '虚拟纹理',
    },

    browse: {
        title: '浏览资源',
        subtitle: '点击表头可按名称或 ID 排序',
        searchPlaceholder: '按名称或 ID 搜索',
        searchClearAria: '清空搜索',
        searchEmpty: '没有匹配 “{term}” 的资源',
        searchClearAction: '清空搜索',
        emptyTitle: '资源尚未扫描',
        emptyDescription: '请先前往“数据库”，选择游戏数据目录并完成扫描。',
        emptyAction: '前往数据库',
    },

    table: {
        headerName: '资源名称',
        headerUuid: 'UUID',
        headerMaterial: '材质',
        headerTexture: '纹理',
        headerVirtual: '虚拟纹理',
        sortByName: '按名称排序',
        sortById: '按 ID 排序',
        empty: '没有可显示的条目',
    },

    pagination: {
        range: '第 {from}-{to} 条 / 共 {total} 条',
        rangeEmpty: '共 0 条',
        first: '首页',
        prev: '上一页',
        next: '下一页',
        last: '末页',
    },

    progress: {
        working: '正在扫描资源…',
    },

    detail: {
        loading: '加载详情…',
        placeholder: '在左侧选择条目查看详情',
        visualAsset: '视觉资源',
        mesh: '网格文件',
        pakLabel: 'PAK 归档',
        materialIds: '材质 · {count}',
        materialLabel: '材质',
        none: '无',
        textures: '纹理 · {count}',
        textureLabel: '纹理',
        noTextures: '该资源没有纹理引用',
        virtualTextures: '虚拟纹理 · {count}',
        virtualTextureLabel: '虚拟纹理',
        noVirtualTextures: '该资源没有虚拟纹理',
    },

    preview: {
        title: '3D 预览',
        loading: '正在转换模型…',
        noMesh: '该资源没有可预览的网格',
        failed: '模型预览生成失败',
    },

    export: {
        action: '导出',
        title: '导出资源',
        emptyName: '未选择资源',
        destLabel: '导出到',
        destEmpty: '尚未选择目录',
        pickDir: '选择目录',
        meshFormatLabel: '网格格式',
        meshFormat: {
            gr2: 'GR2 原始格式',
            gr2Desc: '直接导出包内原始网格，不做任何转换',
            glb: 'GLB 转换格式',
            glbDesc: '转换为通用 GLB 网格，纹理单独导出',
        },
        textureGroup: '纹理',
        textureFormat: {
            none: '无',
            noneDesc: '不导出任何纹理文件',
            dds: 'DDS',
            ddsDesc: '单独导出 DDS 纹理（含虚拟纹理）',
            png: 'PNG',
            pngDesc: '导出 PNG 并丢弃 DDS',
        },
        start: '开始导出',
        running: '导出中…',
        close: '关闭',
        again: '再次导出',
        phase: {
            prepare: '准备中…',
            model: '转换模型…',
            modelRaw: '导出原始网格…',
            textures: '导出纹理…',
            virtualTextures: '提取虚拟纹理…',
            manifest: '写入清单…',
            done: '导出完成',
        },
        resultTitle: '已导出 {count} 个文件',
        filesTitle: '导出文件 · {count}',
        warningsTitle: '提示 · {count}',
        warning: {
            textureUnavailable: '纹理不可用：{detail}',
            textureWriteFailed: '纹理写入失败：{detail}',
            pngConvertFailed: 'PNG 转换失败，已保留 DDS：{detail}',
            pngWriteFailed: 'PNG 写入失败：{detail}',
            vtStagingFailed: '虚拟纹理暂存目录不可用：{detail}',
            vtGtpNotFound: '未找到 GTex hash「{detail}」对应的 GTP 文件',
            vtFailed: '虚拟纹理提取失败：{detail}',
            manifestWriteFailed: 'asset.json 写入失败：{detail}',
            manifestSerializeFailed: 'asset.json 序列化失败：{detail}',
        },
    },

    errors: {
        loadList: '资源列表加载失败，请重试',
        loadDetail: '资源详情加载失败，请重试',
        buildFailed: '资源扫描未完成，请重试',
        directoryFailed: '目录设置未生效，请重试',
        general: '操作未完成，请稍后重试',
    },
} satisfies LocaleMessages

export default zhCN
