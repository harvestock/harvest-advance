import { AdminWebsocket, AppWebsocket } from '@holochain/client'

export interface Connection {
  admin: AdminWebsocket
  app: AppWebsocket
  role: string
  zome: string
  info: any
  cellId: any
}

export interface ConnectOptions {
  adminUrl: string
  appUrl: string
  appId: string
  role?: string
  zome?: string
}

/**
 * Connect to the conductor admin interface, issue an app token, attach to the
 * app websocket and authorize per-cell signing credentials (Holochain 0.7).
 */
export async function connect(options: ConnectOptions): Promise<Connection> {
  const role = options.role ?? 'harvest_advance'
  const zome = options.zome ?? 'harvest_advance'

  const admin = await AdminWebsocket.connect({
    url: new URL(options.adminUrl),
  })

  const { token } = await admin.issueAppAuthenticationToken({
    installed_app_id: options.appId,
    expiry_seconds: 3600,
  })

  const app = await AppWebsocket.connect({
    url: new URL(options.appUrl),
    token: Array.from(token),
  })

  const info = await app.appInfo()
  const cellInfo = (info.cell_info as Record<string, any[]>)[role][0]
  const cellId = cellInfo.value?.cell_id ?? cellInfo.cell_id
  await admin.authorizeSigningCredentials(cellId)

  return { admin, app, role, zome, info, cellId }
}

export async function call<T>(
  connection: Connection,
  fnName: string,
  payload?: unknown,
): Promise<T> {
  return connection.app.callZome({
    role_name: connection.role,
    zome_name: connection.zome,
    fn_name: fnName,
    payload,
  }) as Promise<T>
}