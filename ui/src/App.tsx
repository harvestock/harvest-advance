import { useEffect, useRef, useState } from 'react'
import { encodeHashToBase64 } from '@holochain/client'
import { Connection, connect, call } from './holochain'

type Campaign = {
  id: string
  name: string
  state: string
  goal_amount: string
  current_amount: string
  enterprise_id: string
  [key: string]: unknown
}

type Contribution = {
  id: string
  campaign_id: string
  quantity: number
  unit_price: string
  total_amount: string
  plan: string
  paid_amount: string
  [key: string]: unknown
}

type CampaignRow = [string, Campaign]
type ContributionRow = [string, Contribution]

const STATES = ['Active', 'Funded', 'Completed', 'Canceled']

function field(value: string): string {
  return value.trim()
}

function num(value: string): number {
  return Number.parseInt(value, 10)
}

export default function App() {
  const [adminUrl, setAdminUrl] = useState('ws://127.0.0.1:34817')
  const [appUrl, setAppUrl] = useState('ws://127.0.0.1:9000')
  const [appId, setAppId] = useState('harvest-advance')
  const [connection, setConnection] = useState<Connection | null>(null)
  const [status, setStatus] = useState('disconnected')
  const [log, setLog] = useState<string[]>([])

  const [diag, setDiag] = useState<{
    agentKey: string
    appId: string
    dna: string
    cell: string
  } | null>(null)
  const [netStats, setNetStats] = useState('')
  const [metrics, setMetrics] = useState('')
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [tab, setTab] = useState<'business' | 'diag'>('business')
  const [peers, setPeers] = useState('')

  const [campaigns, setCampaigns] = useState<CampaignRow[]>([])
  const [contributions, setContributions] = useState<ContributionRow[]>([])

  const [campaignForm, setCampaignForm] = useState({
    enterprise_id: '',
    name: '',
    goal_amount: '5000.00',
    target_quantity: '100',
    start_at: '1000',
    end_at: '2000',
  })
  const [pledgeForm, setPledgeForm] = useState({
    campaign_id: '',
    quantity: '1',
    unit_price: '0.1',
    plan: 'Full',
  })
  const [stateForm, setStateForm] = useState({ campaign_id: '', state: 'Active' })

  const unsubscribe = useRef<(() => void) | null>(null)
  const didAutoConnect = useRef(false)

  function note(message: string) {
    setLog((current) => [`${new Date().toLocaleTimeString()} ${message}`, ...current].slice(0, 50))
  }

  async function refresh() {
    if (!connection) return
    try {
      const rows = await call<CampaignRow[]>(connection, 'list_campaigns', {
        state: null,
        enterprise_id: null,
        limit: 100,
      })
      setCampaigns(rows)
      const mine = await call<ContributionRow[]>(connection, 'get_my_contributions', null)
      setContributions(mine)
    } catch (error) {
      note(`refresh failed: ${String(error)}`)
    }
  }

  async function connectAndLoad(): Promise<boolean> {
    setStatus('connecting…')
    try {
      const next = await connect({ adminUrl, appUrl, appId })
      setConnection(next)
      const cellId = next.cellId
      setDiag({
        agentKey: next.app.myPubKey ? encodeHashToBase64(next.app.myPubKey) : '?',
        appId: String(next.info?.installed_app_id ?? appId),
        dna: cellId ? encodeHashToBase64(cellId[0]) : '?',
        cell: cellId ? encodeHashToBase64(cellId[1]) : '?',
      })
      setStatus(
        `connected as ${next.app.myPubKey ? encodeHashToBase64(next.app.myPubKey) : 'unknown'}`,
      )
      note('connected')
      unsubscribe.current?.()
      unsubscribe.current = next.app.on('signal', (signal: unknown) => {
        note(`signal: ${JSON.stringify(signal)}`)
        void refresh()
      })
      await refresh()
      void refreshDiagnostics(next)
      return true
    } catch (error) {
      setStatus(`connection failed: ${String(error)}`)
      return false
    }
  }

  async function refreshDiagnostics(conn: Connection | null = connection) {
    if (!conn) return
    try {
      const stats = await conn.admin.dumpNetworkStats()
      const t = stats.transport_stats ?? {}
      const urls: string[] = t.peer_urls ?? []
      const connections = t.connections ?? []
      const blocked = Object.keys(stats.blocked_message_counts ?? {}).length
      setNetStats(
        [
          `backend: ${t.backend ?? '?'}`,
          `peer_urls: ${urls.length ? urls.join(', ') : '(none yet)'}`,
          `connections: ${connections.length}`,
          `blocked_message_counts: ${blocked}`,
        ].join('\n'),
      )
    } catch (error) {
      setNetStats(`error: ${String(error)}`)
    }
    try {
      const m = await conn.admin.dumpNetworkMetrics({
        dna_hash: conn.cellId?.[0],
        include_dht_summary: true,
      })
      setMetrics(JSON.stringify(m, null, 2).slice(0, 6000))
    } catch (error) {
      setMetrics(`error: ${String(error)}`)
    }
    try {
      const agents = await conn.admin.agentInfo({ dna_hashes: null })
      setPeers(
        Array.isArray(agents) && agents.length
          ? agents.join('\n')
          : '(no known agents yet)',
      )
    } catch (error) {
      setPeers(`error: ${String(error)}`)
    }
  }

  useEffect(() => {
    return () => {
      unsubscribe.current?.()
    }
  }, [])

  // Auto-connect on load (retry briefly while the conductor warms up).
  useEffect(() => {
    if (didAutoConnect.current) return
    didAutoConnect.current = true
    let cancelled = false
    const attempt = async () => {
      for (let i = 0; i < 15 && !cancelled; i++) {
        const ok = await connectAndLoad()
        if (ok || cancelled) return
        await new Promise((resolve) => setTimeout(resolve, 1000))
      }
    }
    void attempt()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!autoRefresh || !connection) return
    void refreshDiagnostics(connection)
    const id = setInterval(() => void refreshDiagnostics(connection), 5000)
    return () => clearInterval(id)
  }, [autoRefresh, connection])

  async function run(label: string, fn: () => Promise<unknown>) {
    try {
      const result = await fn()
      note(`${label} ok: ${JSON.stringify(result)}`)
      await refresh()
    } catch (error) {
      note(`${label} failed: ${String(error)}`)
    }
  }

  async function submitCampaign(event: React.FormEvent) {
    event.preventDefault()
    if (!connection) return
    await run('create_campaign', () =>
      call(connection, 'create_campaign', {
        enterprise_id: field(campaignForm.enterprise_id),
        product_variant_id: '',
        name: field(campaignForm.name),
        description: null,
        goal_amount: field(campaignForm.goal_amount),
        target_quantity: num(campaignForm.target_quantity),
        start_at: num(campaignForm.start_at),
        end_at: num(campaignForm.end_at),
      }),
    )
  }

  async function submitPledge(event: React.FormEvent) {
    event.preventDefault()
    if (!connection) return
    await run('pledge_to_campaign', () =>
      call(connection, 'pledge_to_campaign', {
        campaign_id: field(pledgeForm.campaign_id),
        product_variant_id: '',
        quantity: num(pledgeForm.quantity),
        unit_price: field(pledgeForm.unit_price),
        plan: pledgeForm.plan,
      }),
    )
  }

  async function submitState(event: React.FormEvent) {
    event.preventDefault()
    if (!connection) return
    await run('set_campaign_state', () =>
      call(connection, 'set_campaign_state', {
        campaign_id: field(stateForm.campaign_id),
        state: stateForm.state,
      }),
    )
  }

  return (
    <main>
      <h1>Harvest Advance</h1>

      <div className="tabs">
        <button className={tab === 'business' ? 'active' : ''} onClick={() => setTab('business')}>
          Business
        </button>
        <button className={tab === 'diag' ? 'active' : ''} onClick={() => setTab('diag')}>
          Diagnostics &amp; Logs
        </button>
      </div>

      {tab === 'business' && (<>
      <section>
        <h2>Create campaign</h2>
        <form onSubmit={submitCampaign} className="grid">
          <label>
            Enterprise id
            <input
              value={campaignForm.enterprise_id}
              onChange={(e) => setCampaignForm({ ...campaignForm, enterprise_id: e.target.value })}
            />
          </label>
          <label>
            Name
            <input
              value={campaignForm.name}
              onChange={(e) => setCampaignForm({ ...campaignForm, name: e.target.value })}
            />
          </label>
          <label>
            Goal amount
            <input
              value={campaignForm.goal_amount}
              onChange={(e) => setCampaignForm({ ...campaignForm, goal_amount: e.target.value })}
            />
          </label>
          <label>
            Target quantity
            <input
              value={campaignForm.target_quantity}
              onChange={(e) =>
                setCampaignForm({ ...campaignForm, target_quantity: e.target.value })
              }
            />
          </label>
          <label>
            Start at (s)
            <input
              value={campaignForm.start_at}
              onChange={(e) => setCampaignForm({ ...campaignForm, start_at: e.target.value })}
            />
          </label>
          <label>
            End at (s)
            <input
              value={campaignForm.end_at}
              onChange={(e) => setCampaignForm({ ...campaignForm, end_at: e.target.value })}
            />
          </label>
          <button disabled={!connection}>Create</button>
        </form>
      </section>

      <section>
        <h2>Campaigns</h2>
        <button onClick={() => void refresh()} disabled={!connection}>
          Refresh
        </button>
        <table>
          <thead>
            <tr>
              <th>Address</th>
              <th>Id</th>
              <th>Name</th>
              <th>Goal</th>
              <th>State</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {campaigns.map(([address, campaign]) => (
              <tr key={address}>
                <td className="mono">{address.slice(0, 18)}…</td>
                <td className="mono">{campaign.id}</td>
                <td>{campaign.name}</td>
                <td>{campaign.goal_amount}</td>
                <td>{campaign.state}</td>
                <td>
                  <button
                    onClick={() =>
                      setPledgeForm({ ...pledgeForm, campaign_id: campaign.id })
                    }
                  >
                    Pledge
                  </button>{' '}
                  <button
                    onClick={() =>
                      setStateForm({ campaign_id: campaign.id, state: 'Active' })
                    }
                  >
                    Set state
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section>
        <h2>Pledge</h2>
        <form onSubmit={submitPledge} className="grid">
          <label>
            Campaign id
            <input
              value={pledgeForm.campaign_id}
              onChange={(e) => setPledgeForm({ ...pledgeForm, campaign_id: e.target.value })}
            />
          </label>
          <label>
            Quantity
            <input
              value={pledgeForm.quantity}
              onChange={(e) => setPledgeForm({ ...pledgeForm, quantity: e.target.value })}
            />
          </label>
          <label>
            Unit price
            <input
              value={pledgeForm.unit_price}
              onChange={(e) => setPledgeForm({ ...pledgeForm, unit_price: e.target.value })}
            />
          </label>
          <label>
            Plan
            <select
              value={pledgeForm.plan}
              onChange={(e) => setPledgeForm({ ...pledgeForm, plan: e.target.value })}
            >
              <option value="Full">Full</option>
              <option value="Installments">Installments</option>
            </select>
          </label>
          <button disabled={!connection}>Pledge</button>
        </form>
      </section>

      <section>
        <h2>Set campaign state</h2>
        <form onSubmit={submitState} className="grid">
          <label>
            Campaign id
            <input
              value={stateForm.campaign_id}
              onChange={(e) => setStateForm({ ...stateForm, campaign_id: e.target.value })}
            />
          </label>
          <label>
            State
            <select
              value={stateForm.state}
              onChange={(e) => setStateForm({ ...stateForm, state: e.target.value })}
            >
              {STATES.map((state) => (
                <option key={state} value={state}>
                  {state}
                </option>
              ))}
            </select>
          </label>
          <button disabled={!connection}>Update</button>
        </form>
      </section>

      <section>
        <h2>My contributions</h2>
        <table>
          <thead>
            <tr>
              <th>Id</th>
              <th>Campaign</th>
              <th>Quantity</th>
              <th>Unit price</th>
              <th>Total</th>
              <th>Plan</th>
              <th>Paid</th>
            </tr>
          </thead>
          <tbody>
            {contributions.map(([address, contribution]) => (
              <tr key={address}>
                <td className="mono">{contribution.id}</td>
                <td className="mono">{contribution.campaign_id}</td>
                <td>{contribution.quantity}</td>
                <td>{contribution.unit_price}</td>
                <td>{contribution.total_amount}</td>
                <td>{contribution.plan}</td>
                <td>{contribution.paid_amount}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      </>)}

      {tab === 'diag' && (<>
        <details className="advanced">
          <summary>Connection settings</summary>
          <div className="row">
            <label>
              Admin URL
              <input value={adminUrl} onChange={(e) => setAdminUrl(e.target.value)} />
            </label>
            <label>
              App URL
              <input value={appUrl} onChange={(e) => setAppUrl(e.target.value)} />
            </label>
            <label>
              App ID
              <input value={appId} onChange={(e) => setAppId(e.target.value)} />
            </label>
            <button
              onClick={() => void connectAndLoad()}
              disabled={!!connection || status.startsWith('connecting')}
            >
              {connection ? 'Connected' : 'Connect'}
            </button>
          </div>
          <p className="status">{status}</p>
        </details>

        <section>
          <h2>Diagnostics</h2>
          {diag ? (
            <div className="grid">
              <label>
                Agent key
                <code className="mono">{diag.agentKey}</code>
              </label>
              <label>
                App id
                <code className="mono">{diag.appId}</code>
              </label>
              <label>
                DNA hash
                <code className="mono">{diag.dna}</code>
              </label>
              <label>
                Cell (agent)
                <code className="mono">{diag.cell}</code>
              </label>
            </div>
          ) : (
            <p className="status">Connect to see identity and DHT diagnostics.</p>
          )}
          <div className="row">
            <button onClick={() => void refreshDiagnostics()} disabled={!connection}>
              Refresh DHT stats
            </button>
            <label className="inline">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(e) => setAutoRefresh(e.target.checked)}
              />{' '}
              Auto-refresh (5s)
            </label>
          </div>
          <h3>Network / DHT</h3>
          <pre className="log">{netStats || '(no data — connect and refresh)'}</pre>
          <h3>Network metrics (per DNA)</h3>
          <pre className="log">{metrics || '(no data)'}</pre>
          <h3>Peers / known agents</h3>
          <pre className="log">{peers || '(no data)'}</pre>
        </section>

        <section>
          <h2>Logs</h2>
          <div className="row">
            <button onClick={() => navigator.clipboard?.writeText(log.join('\n'))}>Copy</button>
            <button onClick={() => setLog([])}>Clear</button>
          </div>
          <pre className="log">{log.join('\n')}</pre>
        </section>
      </>)}
    </main>
  )
}