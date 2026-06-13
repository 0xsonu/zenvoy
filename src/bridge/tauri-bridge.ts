import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ZenBridge, ZenCapabilities, ZenAppInfo } from './contract'
import type { CustomTemplateFile, WriteTemplateInput } from '@bridge/templates'
import type {
  AppUpdateState,
  AssetMeta,
  DeletedAsset,
  ExternalFileContent,
  FolderEntry,
  ImportedAsset,
  LocalVaultEntry,
  MoveExternalFileResult,
  ListNotesPageRequest,
  ListNotesPageResponse,
  NoteComment,
  NoteCommentInput,
  NoteContent,
  NoteFolder,
  NoteMeta,
  PastedImageInput,
  RaycastExtensionStatus,
  DirectoryBrowseResult,
  RemoteWorkspaceInfo,
  RemoteWorkspaceProfile,
  RemoteWorkspaceProfileInput,
  ServerCapabilities,
  ServerSessionStatus,
  VaultSettings,
  TikzRenderResponse,
  VaultChangeEvent,
  VaultDemoTourResult,
  VaultInfo,
  VaultTextSearchBackendPreference,
  VaultTextSearchCapabilities,
  VaultTextSearchMatch,
  VaultTextSearchToolPaths,
  CliInstallStatus
} from '@shared/ipc'
import type { VaultTask } from '@shared/tasks'
import type {
  DatabaseDoc,
  DatabaseSidecar,
  DatabaseSummary,
  DbRow
} from '@shared/databases'
import type {
  McpClientId,
  McpClientStatus,
  McpInstructionsPayload,
  McpServerRuntime
} from '@shared/mcp-clients'

const TAURI_CAPABILITIES: ZenCapabilities = {
  supportsUpdater: true,
  supportsNativeMenus: false,
  supportsFloatingWindows: true,
  supportsLocalFilesystemPickers: true,
  supportsRemoteWorkspace: true,
  supportsCliInstall: true,
  supportsCustomTemplates: true,
}

const TAURI_APP_INFO: ZenAppInfo = {
  name: 'zenvoy',
  productName: 'Zenvoy',
  version: '1.0.0',
  description: 'Keyboard-first Markdown notes',
  runtime: 'desktop',
}

function tauriListen<T>(event: string, cb: (payload: T) => void): () => void {
  let unlisten: UnlistenFn | null = null
  listen<T>(event, (e) => cb(e.payload)).then((fn) => (unlisten = fn))
  return () => { unlisten?.() }
}

export function createTauriBridge(): ZenBridge {
  return {
    // ── App info & capabilities ──────────────────────────────────────
    getCapabilities: () => TAURI_CAPABILITIES,
    getAppInfo: () => TAURI_APP_INFO,

    // ── Platform ─────────────────────────────────────────────────────
    platform: () => invoke<NodeJS.Platform>('platform'),
    platformSync: () => 'darwin' as NodeJS.Platform,
    listSystemFonts: () => invoke<string[]>('list_system_fonts'),
    getAppIconDataUrl: () => invoke<string | null>('get_app_icon_data_url'),

    // ── Zoom ─────────────────────────────────────────────────────────
    zoomInApp: () => invoke<number>('zoom_in_app'),
    zoomOutApp: () => invoke<number>('zoom_out_app'),
    resetAppZoom: () => invoke<number>('reset_app_zoom'),

    // ── Updater ──────────────────────────────────────────────────────
    getAppUpdateState: () => invoke<AppUpdateState>('get_app_update_state'),
    checkForAppUpdates: () => invoke<AppUpdateState>('check_for_app_updates'),
    checkForAppUpdatesWithUi: () => invoke<void>('check_for_app_updates_with_ui'),
    downloadAppUpdate: () => invoke<AppUpdateState>('download_app_update'),
    installAppUpdate: () => invoke<void>('install_app_update'),

    // ── Server session ───────────────────────────────────────────────
    getServerCapabilities: () => invoke<ServerCapabilities | null>('get_server_capabilities'),
    getServerSession: () => invoke<ServerSessionStatus>('get_server_session'),
    loginServerSession: (token) => invoke<ServerSessionStatus>('login_server_session', { token }),
    logoutServerSession: () => invoke<ServerSessionStatus>('logout_server_session'),

    // ── Remote workspace ─────────────────────────────────────────────
    getRemoteWorkspaceInfo: () => invoke<RemoteWorkspaceInfo | null>('get_remote_workspace_info'),
    connectRemoteWorkspace: (baseUrl, authToken) =>
      invoke<{ vault: VaultInfo | null; capabilities: ServerCapabilities }>('connect_remote_workspace', { baseUrl, authToken }),
    disconnectRemoteWorkspace: () => invoke<VaultInfo | null>('disconnect_remote_workspace'),
    listRemoteWorkspaceProfiles: () => invoke<RemoteWorkspaceProfile[]>('list_remote_workspace_profiles'),
    saveRemoteWorkspaceProfile: (input) => invoke<RemoteWorkspaceProfile>('save_remote_workspace_profile', { input }),
    deleteRemoteWorkspaceProfile: (id) => invoke<void>('delete_remote_workspace_profile', { id }),
    connectRemoteWorkspaceProfile: (id) =>
      invoke<{ vault: VaultInfo | null; capabilities: ServerCapabilities }>('connect_remote_workspace_profile', { id }),

    // ── Vault management ─────────────────────────────────────────────
    getCurrentVault: () => invoke<VaultInfo | null>('get_current_vault'),
    listLocalVaults: () => invoke<LocalVaultEntry[]>('list_local_vaults'),
    openLocalVault: (root) => invoke<VaultInfo | null>('open_local_vault', { root }),
    closeVault: () => invoke<VaultInfo | null>('close_vault'),
    pickVault: () => invoke<VaultInfo | null>('pick_vault'),
    selectVaultPath: (path) => invoke<VaultInfo>('select_vault_path', { path }),
    browseServerDirectories: (path) => invoke<DirectoryBrowseResult>('browse_server_directories', { path }),
    getVaultSettings: () => invoke<VaultSettings>('get_vault_settings'),
    setVaultSettings: (next) => invoke<VaultSettings>('set_vault_settings', { next }),

    // ── Notes ────────────────────────────────────────────────────────
    listNotes: () => invoke<NoteMeta[]>('list_notes'),
    listNotesPage: undefined,
    listFolders: () => invoke<FolderEntry[]>('list_folders'),
    listAssets: () => invoke<AssetMeta[]>('list_assets'),
    hasAssetsDir: () => invoke<boolean>('has_assets_dir'),
    generateDemoTour: () => invoke<VaultDemoTourResult>('generate_demo_tour'),
    removeDemoTour: () => invoke<VaultDemoTourResult>('remove_demo_tour'),

    // ── Templates ────────────────────────────────────────────────────
    listTemplates: () => invoke<CustomTemplateFile[]>('list_templates'),
    readTemplate: (sourcePath) => invoke<string>('read_template', { sourcePath }),
    writeTemplate: (input) => invoke<CustomTemplateFile>('write_template', { input }),
    deleteTemplate: (sourcePath) => invoke<void>('delete_template', { sourcePath }),

    // ── Search ───────────────────────────────────────────────────────
    getVaultTextSearchCapabilities: (paths) =>
      invoke<VaultTextSearchCapabilities>('get_vault_text_search_capabilities', { paths }),
    searchVaultText: (query, backend, paths) =>
      invoke<VaultTextSearchMatch[]>('search_vault_text', { query, backend, paths }),

    // ── Note CRUD ────────────────────────────────────────────────────
    readNote: (relPath) => invoke<NoteContent>('read_note', { relPath }),
    readNoteComments: (relPath) => invoke<NoteComment[]>('read_note_comments', { relPath }),
    writeNoteComments: (relPath, comments) => invoke<NoteComment[]>('write_note_comments', { relPath, comments }),
    scanTasks: () => invoke<VaultTask[]>('scan_tasks'),
    scanTasksForPath: (relPath) => invoke<VaultTask[]>('scan_tasks_for_path', { relPath }),

    // ── Databases ────────────────────────────────────────────────────
    openDatabase: (relPath) => invoke<DatabaseDoc>('open_database', { relPath }),
    writeDatabaseRows: (relPath, rows) => invoke<DatabaseDoc>('write_database_rows', { relPath, rows }),
    writeDatabaseSchema: (relPath, sidecar, rows) => invoke<DatabaseDoc>('write_database_schema', { relPath, sidecar, rows }),
    createDatabase: (folder, subpath, title) => invoke<DatabaseDoc>('create_database', { folder, subpath, title }),
    createRecordPage: (csvPath, title, body) => invoke<string>('create_record_page', { csvPath, title, body }),
    listDatabases: () => invoke<DatabaseSummary[]>('list_databases'),

    // ── Note mutations ───────────────────────────────────────────────
    writeNote: (relPath, body) => invoke<NoteMeta>('write_note', { relPath, body }),
    appendToNote: (relPath, body, position) => invoke<NoteMeta>('append_to_note', { relPath, body, position }),
    createNote: (folder, title, subpath) => invoke<NoteMeta>('create_note', { folder, title, subpath }),
    renameNote: (relPath, nextTitle) => invoke<NoteMeta>('rename_note', { relPath, nextTitle }),
    deleteNote: (relPath) => invoke<void>('delete_note', { relPath }),
    moveToTrash: (relPath) => invoke<NoteMeta>('move_to_trash', { relPath }),
    restoreFromTrash: (relPath) => invoke<NoteMeta>('restore_from_trash', { relPath }),
    emptyTrash: () => invoke<void>('empty_trash'),
    archiveNote: (relPath) => invoke<NoteMeta>('archive_note', { relPath }),
    unarchiveNote: (relPath) => invoke<NoteMeta>('unarchive_note', { relPath }),
    duplicateNote: (relPath) => invoke<NoteMeta>('duplicate_note', { relPath }),
    exportNotePdf: (relPath) => invoke<string | null>('export_note_pdf', { relPath }),
    revealNote: (relPath) => invoke<void>('reveal_note', { relPath }),
    revealNoteTarget: (relPath) => invoke<void>('reveal_note_target', { relPath }),
    moveNote: (relPath, targetFolder, targetSubpath) =>
      invoke<NoteMeta>('move_note', { relPath, targetFolder, targetSubpath }),

    // ── Assets ───────────────────────────────────────────────────────
    importFilesToNote: (notePath, sourcePaths) => invoke<ImportedAsset[]>('import_files_to_note', { notePath, sourcePaths }),
    importPastedImage: (input) => invoke<ImportedAsset>('import_pasted_image', { input }),
    renameAsset: (relPath, nextName) => invoke<AssetMeta>('rename_asset', { relPath, nextName }),
    moveAsset: (relPath, targetDir) => invoke<AssetMeta>('move_asset', { relPath, targetDir }),
    duplicateAsset: (relPath) => invoke<AssetMeta>('duplicate_asset', { relPath }),
    deleteAsset: (relPath) => invoke<DeletedAsset>('delete_asset', { relPath }),
    restoreDeletedAsset: (asset) => invoke<AssetMeta>('restore_deleted_asset', { asset }),

    // ── Folders ──────────────────────────────────────────────────────
    createFolder: (folder, subpath) => invoke<void>('create_folder', { folder, subpath }),
    renameFolder: (folder, oldSubpath, newSubpath) => invoke<string>('rename_folder', { folder, oldSubpath, newSubpath }),
    deleteFolder: (folder, subpath) => invoke<void>('delete_folder', { folder, subpath }),
    duplicateFolder: (folder, subpath) => invoke<string>('duplicate_folder', { folder, subpath }),
    revealFolder: (folder, subpath) => invoke<void>('reveal_folder', { folder, subpath }),
    revealFolderTarget: (folder, subpath) => invoke<void>('reveal_folder_target', { folder, subpath }),
    revealAssetsDir: () => invoke<void>('reveal_assets_dir'),

    // ── File path helpers ────────────────────────────────────────────
    getPathForFile: (_file) => null,
    resolveLocalAssetUrl: (vaultRoot, notePath, href) =>
      `asset://localhost/${encodeURIComponent(vaultRoot)}/${notePath}/../${href}`,
    resolveVaultAssetUrl: (vaultRoot, assetPath) =>
      `asset://localhost/${encodeURIComponent(vaultRoot)}/${assetPath}`,

    // ── Events ───────────────────────────────────────────────────────
    onVaultChange: (cb) => tauriListen<VaultChangeEvent>('vault-change', cb),
    onOpenSettings: (cb) => tauriListen<void>('open-settings', () => cb()),
    onOpenNoteRequested: (cb) => tauriListen<string>('open-note-requested', cb),
    notifyRendererReady: () => { invoke('notify_renderer_ready') },
    onAppUpdateState: (cb) => tauriListen<AppUpdateState>('app-update-state', cb),

    // ── Window management ────────────────────────────────────────────
    windowMinimize: () => { invoke('window_minimize') },
    windowToggleMaximize: () => { invoke('window_toggle_maximize') },
    windowClose: () => { invoke('window_close') },
    openNoteWindow: (relPath) => invoke<void>('open_note_window', { relPath }),
    openVaultWindow: () => invoke<VaultInfo | null>('open_vault_window'),

    // ── External files ───────────────────────────────────────────────
    readExternalFile: () => invoke<ExternalFileContent>('read_external_file'),
    writeExternalFile: (body) => invoke<void>('write_external_file', { body }),
    moveExternalFileToVault: () => invoke<MoveExternalFileResult>('move_external_file_to_vault'),
    openMarkdownFile: (absPath) => invoke<boolean>('open_markdown_file', { absPath }),

    // ── Quick capture ────────────────────────────────────────────────
    toggleQuickCapture: () => invoke<void>('toggle_quick_capture'),
    getQuickCaptureHotkey: () => invoke<string>('get_quick_capture_hotkey'),
    setQuickCaptureHotkey: (hotkey) => invoke<{ ok: boolean; hotkey: string; error?: string }>('set_quick_capture_hotkey', { hotkey }),
    getQuickCapturePinned: () => invoke<boolean>('get_quick_capture_pinned'),
    setQuickCapturePinned: (pinned) => invoke<boolean>('set_quick_capture_pinned', { pinned }),

    // ── TikZ rendering ───────────────────────────────────────────────
    renderTikz: (source) => invoke<TikzRenderResponse>('render_tikz', { source }),

    // ── MCP ──────────────────────────────────────────────────────────
    mcpGetRuntime: () => invoke<McpServerRuntime>('mcp_get_runtime'),
    mcpGetStatuses: () => invoke<McpClientStatus[]>('mcp_get_statuses'),
    mcpInstall: (id) => invoke<McpClientStatus>('mcp_install', { id }),
    mcpUninstall: (id) => invoke<McpClientStatus>('mcp_uninstall', { id }),
    mcpGetInstructions: () => invoke<McpInstructionsPayload>('mcp_get_instructions'),
    mcpSetInstructions: (next) => invoke<McpInstructionsPayload>('mcp_set_instructions', { next }),

    // ── CLI & Raycast ────────────────────────────────────────────────
    cliGetStatus: () => invoke<CliInstallStatus>('cli_get_status'),
    cliInstall: () => invoke<CliInstallStatus>('cli_install'),
    cliUninstall: () => invoke<CliInstallStatus>('cli_uninstall'),
    raycastGetStatus: () => invoke<RaycastExtensionStatus>('raycast_get_status'),
    raycastInstall: () => invoke<RaycastExtensionStatus>('raycast_install'),

    // ── Clipboard ────────────────────────────────────────────────────
    clipboardWriteText: (text) => { invoke('clipboard_write_text', { text }) },
    clipboardReadText: () => '',
  }
}
