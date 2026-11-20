// Conductor smoke test for the Harvest Advance hApp.
//
// Prerequisites: a conductor running with this hApp installed, reachable at
// ADMIN_URL, with an app interface at APP_URL and installed app id APP_ID.
// Defaults match:
//   hc sandbox -f=34817 ... generate -r=9000 --app-id=harvest-advance ...
//
//   node smoke.mjs

import { AdminWebsocket, AppWebsocket, encodeHashToBase64, fakeActionHash } from "@holochain/client";

const ADMIN_URL = process.env.ADMIN_URL ?? "ws://127.0.0.1:34817";
const APP_URL = process.env.APP_URL ?? "ws://127.0.0.1:9000";
const APP_ID = process.env.APP_ID ?? "harvest-advance";
const ROLE = "harvest_advance";
const ZOME = "harvest_advance";

let passed = 0;
let failed = 0;
const failures = [];

function check(name, condition, detail) {
  if (condition) {
    passed += 1;
    console.log(`PASS ${name}`);
  } else {
    failed += 1;
    failures.push(name);
    console.log(`FAIL ${name}${detail ? ` :: ${detail}` : ""}`);
  }
}

function hasSignal(signals, name) {
  return signals.some((s) => s?.value?.payload?.[name]);
}

async function main() {
  const admin = await AdminWebsocket.connect({
    url: ADMIN_URL,
    wsClientOptions: { origin: ADMIN_URL.replace("ws://", "http://") },
  });
  const { token } = await admin.issueAppAuthenticationToken({
    installed_app_id: APP_ID,
    expiry_seconds: 3600,
  });
  const app = await AppWebsocket.connect({
    url: APP_URL,
    token: new Uint8Array(token),
    wsClientOptions: { origin: APP_URL.replace("ws://", "http://") },
  });

  // Holochain 0.7 signs zome calls with per-cell signing credentials.
  const appInfo = await app.appInfo();
  const cellInfo = appInfo.cell_info[ROLE][0];
  const cellId = cellInfo.value?.cell_id ?? cellInfo.cell_id;
  await admin.authorizeSigningCredentials(cellId);

  const signals = [];
  app.on("signal", (signal) => signals.push(signal));

  const call = (fn_name, payload) =>
    app.callZome({ role_name: ROLE, zome_name: ZOME, fn_name, payload });

  const callOk = async (fn_name, payload) => {
    try {
      return { ok: true, value: await call(fn_name, payload) };
    } catch (error) {
      return { ok: false, error };
    }
  };

  // 1. Profile, and one-profile-per-agent (TC-29).
  const profile1 = await callOk("create_profile", {
    email: "grower@example.test",
    name: "Grower",
  });
  check("create_profile succeeds", profile1.ok, String(profile1.error));
  const profile2 = await callOk("create_profile", {
    email: "grower@example.test",
    name: "Grower",
  });
  check("create_profile twice is rejected (TC-29)", !profile2.ok);

  // 2. Enterprise.
  const enterprise = await callOk("create_enterprise", {
    name: "Acme Farm",
    enterprise_type: "farm",
    description: "A test farm",
    permalink: "acme",
    visible: true,
  });
  check("create_enterprise succeeds", enterprise.ok, String(enterprise.error));

  // 3. Campaign.
  const campaign = await callOk("create_campaign", {
    enterprise_id: "enterprise-that-does-not-exist",
    product_variant_id: "pv-1",
    name: "Harvest Advance",
    description: "Pre-finance the harvest",
    goal_amount: "5000.00",
    target_quantity: 100,
    start_at: 1_000,
    end_at: 2_000,
  });
  check("create_campaign succeeds", campaign.ok, String(campaign.error));
  const campaignAddress = campaign.ok ? campaign.value : null;

  const list1 = await callOk("list_campaigns", {
    state: null,
    enterprise_id: null,
    limit: 50,
  });
  check("list_campaigns returns an array", list1.ok && Array.isArray(list1.value));
  const listed = list1.ok
    ? list1.value.find(([address]) => address === campaignAddress)
    : undefined;
  check(
    "list_campaigns sees the campaign from the shared anchor (TC-47)",
    Boolean(listed),
  );
  const campaignId = listed ? listed[1].id : "unknown";

  const fetched = await callOk("get_campaign", campaignAddress);
  check(
    "get_campaign returns the created campaign",
    fetched.ok && fetched.value && fetched.value[1].state === "Draft",
    JSON.stringify(fetched.value),
  );

  // 4. Pledge with exact decimal math (TC-20).
  const pledge = await callOk("pledge_to_campaign", {
    campaign_id: campaignId,
    product_variant_id: "pv-1",
    quantity: 3,
    unit_price: "0.1",
    plan: "Full",
  });
  check("pledge_to_campaign succeeds", pledge.ok, String(pledge.error));

  const mine = await callOk("get_my_contributions", null);
  const contribution = mine.ok && mine.value.length > 0 ? mine.value[0][1] : null;
  check(
    "pledge stores exact decimal total 0.1 * 3 == 0.30 (TC-20)",
    contribution && contribution.total_amount === "0.30",
    contribution ? contribution.total_amount : "no contribution",
  );

  // 5. State transition (TC-32, TC-33, TC-34, TC-38).
  const state1 = await callOk("set_campaign_state", {
    campaign_id: campaignId,
    state: "Active",
  });
  check(
    "set_campaign_state Draft -> Active succeeds (TC-33)",
    state1.ok,
    String(state1.error),
  );

  const after = await callOk("get_campaign", campaignAddress);
  check(
    "get_campaign address is unchanged after transition (TC-38)",
    after.ok && after.value[0] === campaignAddress,
  );
  check(
    "get_campaign reflects the new state (TC-32)",
    after.ok && after.value[1].state === "Active",
  );

  const badState = await callOk("set_campaign_state", {
    campaign_id: campaignId,
    state: "Completed",
  });
  check(
    "set_campaign_state Active -> Completed is rejected (TC-34)",
    !badState.ok,
  );

  // 6. Batch + report (TC-23, TC-24).
  const batch = await callOk("create_batch", {
    campaign_id: campaignId,
    name: "Carrot batch",
    crop: "Carrot",
    variety: "Nantes",
    target_quantity: "100",
    unit: "kg",
    sowing_date: 1_000,
    expected_harvest_at: 2_000,
  });
  check("create_batch succeeds (TC-23)", batch.ok, String(batch.error));
  const batchId = batch.ok ? batch.value : "unknown";

  const report = await callOk("post_batch_report", {
    batch_id: batchId,
    stage: "Sown",
    health: "Good",
    expected_yield: "500",
    actual_yield: null,
    notes: "all good",
  });
  check("post_batch_report succeeds (TC-24)", report.ok, String(report.error));

  const reports = await callOk("get_batch_reports", batchId);
  check(
    "get_batch_reports returns the posted report (TC-25)",
    reports.ok && reports.value.length === 1 && reports.value[0][1].stage === "Sown",
    JSON.stringify(reports.value),
  );

  // 7. Projection + lines (TC-30, TC-31, TC-43, TC-52).
  const projection = await callOk("create_projection", {
    campaign_id: campaignId,
    periods: 6,
    start_month: 0,
    target_margin_pct: "0.10",
    notes: null,
    lines: [
      {
        period_index: 0,
        category: "Seeds",
        description: "Carrot seed",
        quantity: "10",
        unit: "kg",
        unit_cost: "2.00",
        amount: "20.00",
        sort_order: 0,
      },
    ],
  });
  check("create_projection succeeds (TC-31)", projection.ok, String(projection.error));

  const gotProjection = await callOk("get_projection", campaignId);
  check(
    "get_projection finds the linked projection (TC-52)",
    gotProjection.ok && gotProjection.value !== null,
    JSON.stringify(gotProjection.value),
  );

  const lines = await callOk("get_projection_lines", campaignId);
  check(
    "get_projection_lines returns the committed line",
    lines.ok && lines.value.length === 1,
    JSON.stringify(lines.value),
  );

  // 8. Signals (SPEC §5.10 / TC-58, TC-59).
  check("CampaignCreated signal emitted (TC-58)", hasSignal(signals, "CampaignCreated"));
  check(
    "CampaignStateChanged signal emitted (TC-58)",
    hasSignal(signals, "CampaignStateChanged"),
  );
  check(
    "ContributionMade signal emitted (TC-58)",
    hasSignal(signals, "ContributionMade"),
  );
  check(
    "BatchReportPosted signal emitted (TC-58)",
    hasSignal(signals, "BatchReportPosted"),
  );
  check("CampaignFunded is never emitted (TC-59)", !hasSignal(signals, "CampaignFunded"));

  // 9. Negative and shape checks.
  const badCampaigns = await Promise.all([
    callOk("create_campaign", { enterprise_id: "e1", product_variant_id: "pv", name: "", description: null, goal_amount: "100.00", target_quantity: 5, start_at: 1, end_at: 2 }),
    callOk("create_campaign", { enterprise_id: "e1", product_variant_id: "pv", name: "C", description: null, goal_amount: "0", target_quantity: 5, start_at: 1, end_at: 2 }),
    callOk("create_campaign", { enterprise_id: "e1", product_variant_id: "pv", name: "C", description: null, goal_amount: "100.00", target_quantity: 0, start_at: 1, end_at: 2 }),
    callOk("create_campaign", { enterprise_id: "e1", product_variant_id: "pv", name: "C", description: null, goal_amount: "100.00", target_quantity: 5, start_at: 2, end_at: 2 }),
  ]);
  check("create_campaign empty name rejected (TC-13)", !badCampaigns[0].ok);
  check("create_campaign zero goal rejected (TC-12)", !badCampaigns[1].ok);
  check("create_campaign zero target rejected (TC-09)", !badCampaigns[2].ok);
  check("create_campaign end <= start rejected (TC-11)", !badCampaigns[3].ok);

  const fallback = await callOk("create_campaign", { enterprise_id: "unknown-ent", product_variant_id: "pv", name: "Fallback", description: null, goal_amount: "10.00", target_quantity: 1, start_at: 1, end_at: 2 });
  const fallbackGet = fallback.ok ? await callOk("get_campaign", fallback.value) : { ok: false };
  check(
    "create_campaign unknown enterprise falls back to caller owner (TC-14)",
    fallback.ok && fallbackGet.ok && typeof fallbackGet.value[1].owner_id === "string",
  );

  const pledgeInst = await callOk("pledge_to_campaign", { campaign_id: campaignId, product_variant_id: "pv", quantity: 2, unit_price: "1.50", plan: "Installments" });
  check("pledge with Installments plan (TC-17)", pledgeInst.ok);
  const mine2 = await callOk("get_my_contributions", null);
  const inst = mine2.ok ? mine2.value.map(([, c]) => c).find((c) => c.plan === "Installments") : null;
  check(
    "Installments sets 3 total / 0 paid (TC-17)",
    inst && inst.installments_total === 3 && inst.paid_amount === "0" && inst.installments_paid === 0,
  );

  const badPledge1 = await callOk("pledge_to_campaign", { campaign_id: campaignId, product_variant_id: "pv", quantity: 0, unit_price: "1.00", plan: "Full" });
  const badPledge2 = await callOk("pledge_to_campaign", { campaign_id: campaignId, product_variant_id: "pv", quantity: 1, unit_price: "", plan: "Full" });
  check("pledge quantity 0 rejected (TC-18)", !badPledge1.ok);
  check("pledge empty unit_price rejected (TC-19)", !badPledge2.ok);

  check(
    "contribution shape quantity/unit_price/paid_amount (TC-55)",
    contribution && typeof contribution.quantity === "number" && typeof contribution.unit_price === "string" && typeof contribution.paid_amount === "string",
  );

  const draftOnly = await callOk("list_campaigns", { state: "Draft", enterprise_id: null, limit: 50 });
  check(
    "list_campaigns state filter (TC-22)",
    draftOnly.ok && draftOnly.value.every(([, c]) => c.state === "Draft"),
  );
  const byEnt = await callOk("list_campaigns", { state: null, enterprise_id: "enterprise-that-does-not-exist", limit: 50 });
  check(
    "list_campaigns enterprise filter (TC-22)",
    byEnt.ok && byEnt.value.length >= 1 && byEnt.value.every(([, c]) => c.enterprise_id === "enterprise-that-does-not-exist"),
  );

  const unknownState = await callOk("set_campaign_state", { campaign_id: "does-not-exist", state: "Active" });
  check("set_campaign_state unknown id errors (TC-37)", !unknownState.ok);

  // TC-35 / TC-36 on a fresh campaign.
  const c2 = await callOk("create_campaign", { enterprise_id: "e1", product_variant_id: "pv", name: "Second", description: null, goal_amount: "10.00", target_quantity: 1, start_at: 1, end_at: 2 });
  const c2Get = c2.ok ? await callOk("get_campaign", c2.value) : { ok: false };
  const c2Id = c2Get.ok ? c2Get.value[1].id : "unknown";
  await callOk("set_campaign_state", { campaign_id: c2Id, state: "Active" });
  const funded = await callOk("set_campaign_state", { campaign_id: c2Id, state: "Funded" });
  const completed = await callOk("set_campaign_state", { campaign_id: c2Id, state: "Completed" });
  const completedAgain = await callOk("set_campaign_state", { campaign_id: c2Id, state: "Completed" });
  check("Draft -> Active -> Funded -> Completed (TC-33)", funded.ok && completed.ok);
  check("Completed -> Completed is idempotent (TC-36)", completedAgain.ok);

  const canceled = await callOk("set_campaign_state", { campaign_id: campaignId, state: "Canceled" });
  const uncancel = await callOk("set_campaign_state", { campaign_id: campaignId, state: "Active" });
  check("Active -> Canceled (TC-35)", canceled.ok);
  check("Canceled -> Active is rejected (TC-34)", !uncancel.ok);

  const badStateEnum = await callOk("set_campaign_state", { campaign_id: c2Id, state: "Bogus" });
  check("set_campaign_state invalid enum rejected (TC-56)", !badStateEnum.ok);

  check(
    "report_date is set server-side (TC-26)",
    reports.ok && typeof reports.value[0][1].report_date === "number" && reports.value[0][1].report_date > 0,
  );
  check(
    "report actual_yield can be null (TC-27)",
    reports.ok && reports.value[0][1].actual_yield === null,
  );

  const badProj = await callOk("create_projection", { campaign_id: campaignId, periods: 5, start_month: 0, target_margin_pct: "0.1", notes: null, lines: [] });
  check("create_projection periods < 6 rejected (TC-30)", !badProj.ok);
  const emptyProj = await callOk("create_projection", { campaign_id: campaignId, periods: 6, start_month: 0, target_margin_pct: "0.1", notes: null, lines: [] });
  check("create_projection with no lines accepted (TC-53)", emptyProj.ok);

  const badAddr = await callOk("get_campaign", "not-a-valid-address");
  check("get_campaign invalid address errors (TC-49)", !badAddr.ok);
  const nonexistent = await callOk("get_campaign", encodeHashToBase64(await fakeActionHash()));
  check("get_campaign nonexistent address returns null (TC-51)", nonexistent.ok && nonexistent.value === null);

  app.client.close();
  admin.client.close();

  console.log(`\n${passed} passed, ${failed} failed`);
  if (failed > 0) {
    console.log(`Failures: ${failures.join(", ")}`);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(2);
});