import { useAppStore } from '../stores/app-store';

export const translations = {
  en: {
    // General
    dashboard: 'Dashboard',
    servers: 'Servers',
    workshop: 'Workshop',
    community: 'Community',
  settings: 'Settings',
    connect: 'Connect',
    disconnect: 'Disconnect',
    status: 'Status',
    connected: 'Connected',
    disconnected: 'Disconnected',
    connecting: 'Connecting...',

    // Dashboard
    activeServer: 'Active Server',
    noServerSelected: 'No server selected',
    selectServerHint: 'Go to Servers tab to select one',
    proxyMode: 'Proxy Mode',
    systemProxy: 'Proxy',
    tunMode: 'TUN Mode',
    modeDescriptionTun: 'Full device VPN',
    modeDescriptionProxy: 'Browser & App proxy',
    upload: 'Upload',
    download: 'Download',
    speedHistory: 'Speed History',

    // Servers
    addServer: 'Add Server',
    importClipboard: 'Import from Clipboard',
    addSubscription: 'Add Subscription',
    manualServers: 'Manual Servers',
    subscriptions: 'Subscriptions',
    pingMsg: 'Ping',
    ms: 'ms',
    deleteProfile: 'Delete Profile',
    deleteAllProfiles: 'Delete All Manual Profiles',
    updateSub: 'Update Subscription',
    deleteSub: 'Delete Subscription',

    // Workshop
    workshopTitle: 'Workshop',
    discoverRules: 'Discover Routing Rules',
    myRules: 'My Rules',
    communityRules: 'Community Rules',
    install: 'Install',
    uninstall: 'Uninstall',

    // Settings
    preferences: 'Preferences',
    system: 'System',
    launchStartup: 'Launch on Startup (Admin)',
    launchStartupDesc: 'Start with admin privileges at boot (no UAC)',
    autoConnect: 'Auto-connect on Startup',
    autoConnectDesc: 'Automatically connect to last server when app starts',
    socksPort: 'SOCKS5 Port',
    httpPort: 'HTTP Port',
    portChangeHint: 'Reconnect to apply port changes',
    language: 'Language',
    
    coreEngine: 'Core Engine',
    dns: 'DNS',
    l3Stack: 'L3 Stack',
    strictRoute: 'Strict Route',
    strictRouteDesc: 'Force DNS through VPN',
    killSwitch: 'Kill Switch',
    killSwitchDesc: 'Block internet if VPN disconnects',

    data: 'Data',
    clearLogs: 'Clear Logs',
    clearLogsDesc: 'Wipes connection history',
    factoryReset: 'Factory Reset',
    factoryResetDesc: 'Delete all servers & configs',

    checkForUpdates: 'Check for Updates',
    coreInfo: 'Core: sing-box + xray-core',
  },
  ru: {
    // General
    dashboard: 'Главная',
    servers: 'Серверы',
    workshop: 'Мастерская',
    community: 'Сообщество',
  settings: 'Настройки',
    connect: 'Подключить',
    disconnect: 'Отключить',
    status: 'Статус',
    connected: 'Подключено',
    disconnected: 'Отключено',
    connecting: 'Подключение...',

    // Dashboard
    activeServer: 'Активный сервер',
    noServerSelected: 'Сервер не выбран',
    selectServerHint: 'Перейдите во вкладку Серверы',
    proxyMode: 'Режим прокси',
    systemProxy: 'Прокси',
    tunMode: 'TUN-режим',
    modeDescriptionTun: 'VPN для всего устройства',
    modeDescriptionProxy: 'Прокси для браузера и приложений',
    upload: 'Отдача',
    download: 'Загрузка',
    speedHistory: 'История скорости',

    // Servers
    addServer: 'Добавить сервер',
    importClipboard: 'Импорт из буфера',
    addSubscription: 'Добавить подписку',
    manualServers: 'Добавленные вручную',
    subscriptions: 'Подписки',
    pingMsg: 'Пинг',
    ms: 'мс',
    deleteProfile: 'Удалить профиль',
    deleteAllProfiles: 'Удалить все ручные профили',
    updateSub: 'Обновить подписку',
    deleteSub: 'Удалить подписку',

    // Workshop
    workshopTitle: 'Мастерская',
    discoverRules: 'Поиск правил маршрутизации',
    myRules: 'Мои правила',
    communityRules: 'Правила сообщества',
    install: 'Установить',
    uninstall: 'Удалить',

    // Settings
    preferences: 'Настройки',
    system: 'Система',
    launchStartup: 'Запускать при старте (Админ)',
    launchStartupDesc: 'Запуск с правами админа при старте Windows (без UAC)',
    autoConnect: 'Автоподключение при запуске',
    autoConnectDesc: 'Автоматически подключаться к последнему серверу',
    socksPort: 'SOCKS5 Порт',
    httpPort: 'HTTP Порт',
    portChangeHint: 'Переподключитесь для применения',
    language: 'Язык',
    
    coreEngine: 'Ядро',
    dns: 'DNS',
    l3Stack: 'Сетевой стек L3',
    strictRoute: 'Строгий маршрут',
    strictRouteDesc: 'Перенаправлять DNS через VPN',
    killSwitch: 'Kill Switch',
    killSwitchDesc: 'Блокировать интернет при обрыве VPN',

    data: 'Данные',
    clearLogs: 'Очистить логи',
    clearLogsDesc: 'Вся история подключений',
    factoryReset: 'Сброс до заводских',
    factoryResetDesc: 'Удалить все серверы и подписки',

    checkForUpdates: 'Проверить обновления',
    coreInfo: 'Ядра: sing-box + xray-core',
  },
  zh: {
    // General
    dashboard: '主页',
    servers: '服务器',
    workshop: '创意工坊',
    community: '社区',
  settings: '设置',
    connect: '连接',
    disconnect: '断开',
    status: '状态',
    connected: '已连接',
    disconnected: '未连接',
    connecting: '连接中...',

    // Dashboard
    activeServer: '当前服务器',
    noServerSelected: '未选择服务器',
    selectServerHint: '前往服务器标签页选择',
    proxyMode: '代理模式',
    systemProxy: '代理',
    tunMode: 'TUN模式',
    modeDescriptionTun: '全局设备VPN',
    modeDescriptionProxy: '浏览器和应用代理',
    upload: '上传',
    download: '下载',
    speedHistory: '速度历史',

    // Servers
    addServer: '添加服务器',
    importClipboard: '从剪贴板导入',
    addSubscription: '添加订阅',
    manualServers: '手动添加的服务器',
    subscriptions: '订阅',
    pingMsg: '延迟',
    ms: '毫秒',
    deleteProfile: '删除配置文件',
    deleteAllProfiles: '删除所有手动配置文件',
    updateSub: '更新订阅',
    deleteSub: '删除订阅',

    // Workshop
    workshopTitle: '创意工坊',
    discoverRules: '发现路由规则',
    myRules: '我的规则',
    communityRules: '社区规则',
    install: '安装',
    uninstall: '卸载',

    // Settings
    preferences: '偏好设置',
    system: '系统',
    launchStartup: '开机自启 (管理员)',
    launchStartupDesc: '以管理员权限启动，无需UAC确认',
    autoConnect: '启动时自动连接',
    autoConnectDesc: '应用启动时自动连接到上次使用的服务器',
    socksPort: 'SOCKS5 端口',
    httpPort: 'HTTP 端口',
    portChangeHint: '重新连接以应用端口更改',
    language: '语言',
    
    coreEngine: '核心引擎',
    dns: 'DNS',
    l3Stack: 'L3 堆栈',
    strictRoute: '严格路由',
    strictRouteDesc: '强制 DNS 走代理',
    killSwitch: 'Kill Switch',
    killSwitchDesc: 'VPN断开时阻止互联网访问',

    data: '数据',
    clearLogs: '清除日志',
    clearLogsDesc: '清除连接记录',
    factoryReset: '恢复出厂',
    factoryResetDesc: '删除所有服务器和配置',

    checkForUpdates: '检查更新',
    coreInfo: '核心：sing-box + xray-core',
  }
};

export function useTranslation() {
  const { language } = useAppStore();
  const dict = translations[language as keyof typeof translations] || translations.en;
  
  return {
    t: (key: keyof typeof translations.en) => dict[key] || translations.en[key] || key
  };
}
