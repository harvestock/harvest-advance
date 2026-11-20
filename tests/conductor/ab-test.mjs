// A/B conductor test for Harvest Advance lifecycle processes.
//
// Two agents share one conductor + network:
//   A = campaign owner (producer)
//   B = contributor (buyer)
//
// A (happy path):  create -> pledge -> accept -> pay -> complete
// B (failure paths): decline, fail-to-pay (default), fail-to-deliver
//
// Prerequisites: a conductor with the hApp installed as APP_ID, an app
// interface at APP_URL, and the packed happ at HAPP_PATH (used to install a
// second agent for the contributor).
//
//   ADMIN_URL=ws://127.0.0.1:34818 APP_URL=ws://127.0.0.1:9001 \
//   APP_ID=harvest-advance HAPP_PATH=dist/harvest-advance.happ node ab-test.mjs

import { AdminWebsocket, AppWebsocket } from "@holochain/client";
import path from "node:path";
import { existsSync } from "node:fs";

const ADMIN_URL = process.env.ADMIN_URL ?? "ws://127.0.0.1:34817";
const APP_URL = process.env.APP_URL ?? "ws://127.0.0.1:9000";
const APP_ID = process.env.APP_ID ?? "harvest-advance";
const HAPP_PATH =
  process.env.HAPP_PATH ?? path.resolve(process.cwd(), "dist/harvest-advance.happ");
const ROLE = "harvest_advance";
const ZOME = "harvest_advance";
const B_APP_ID = `${APP_ID}-b`;

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

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function connectApp(admin, appId) {
  const { token } = await admin.issueAppAuthenticationToken({
    installed_app_id: appId,
    expiry_seconds: 3600,
  });
  const app = await AppWebsocket.connect({
    url: APP_URL,
    token: new Uint8Array(token),
    wsClientOptions: { origin: APP_URL.replace("ws://", "http://") },
  });
  const info = await app.appInfo();
  const cellInfo = info.cell_info[ROLE][0];
  const cellId = cellInfo.value?.cell_id ?? cellInfo.cell_id;
  await admin.authorizeSigningCredentials(cellId);

  const call = (fn_name, payload) =>
    app.callZome({ role_name: ROLE, zome_name: ZOME, fn_name, payload });
  const callOk = async (fn_name, payload) => {
    try {
      return { ok: true, value: await call(fn_name, payload) };
    } catch (error) {
      return { ok: false, error };
    }
  };
  return { app, call, callOk };
}

async function connectWithRetry(admin, appId, attempts = 8) {
  let lastError;
  for (let i = 0; i < attempts; i += 1) {
    try {
      return await connectApp(admin, appId);
    } catch (error) {
      lastError = error;
      await sleep(500);
    }
  }
  throw lastError;
}

async function ensureSecondApp(admin) {
  let apps = [];
  try {
    apps = await admin.listApps();
  } catch {
    /* ignore */
  }
  if (!apps.some((app) => app.installed_app_id === B_APP_ID)) {
    if (!existsSync(HAPP_PATH)) {
      throw new Error(`packed happ not found at ${HAPP_PATH}`);
    }
    await admin.installApp({
      source: { type: "path", value: HAPP_PATH },
      installed_app_id: B_APP_ID,
    });
  }
  // installApp leaves the app disabled in Holochain 0.7; enable it.
  try {
    await admin.enableApp({ installed_app_id: B_APP_ID });
  } catch {
    /* already enabled */
  }
  // Give genesis a moment to complete.
  await sleep(750);
}

async function waitFor(checkFn, { attempts = 80, delay = 250, label = "condition" } = {}) {
  for (let i = 0; i < attempts; i += 1) {
    const value = await checkFn();
    if (value) return value;
    await sleep(delay);
  }
  throw new Error(`timed out waiting for ${label}`);
}

async function main() {
  const admin = await AdminWebsocket.connect({
    url: ADMIN_URL,
    wsClientOptions: { origin: ADMIN_URL.replace("ws://", "http://") },
  });

  const A = await connectWithRetry(admin, APP_ID);
  await ensureSecondApp(admin);
  const B = await connectWithRetry(admin, B_APP_ID);

  const newCampaign = async (name, goal) => {
    const res = await A.callOk("create_campaign", {
      enterprise_id: "ab-enterprise",
      product_variant_id: "pv",
      name,
      description: null,
      goal_amount: goal,
      target_quantity: 100,
      start_at: 1_000,
      end_at: 2_000,
    });
    if (!res.ok) throw new Error(`create_campaign failed: ${res.error}`);
    const campaign = await waitFor(
      async () => {
        const get = await A.callOk("get_campaign", res.value);
        return get.ok && get.value ? get.value : null;
      },
      { label: "campaign read" },
    );
    return { address: res.value, id: campaign[1].id };
  };

  const activate = async (campaignId) => {
    const res = await A.callOk("set_campaign_state", {
      campaign_id: campaignId,
      state: "Active",
    });
    if (!res.ok) throw new Error(`activate failed: ${res.error}`);
  };

  const syncToContributor = async (campaignAddress) => {
    await waitFor(
      async () => {
        const get = await B.callOk("get_campaign", campaignAddress);
        return get.ok && get.value ? get.value : null;
      },
      { label: "contributor sees campaign" },
    );
  };

  const pledgeFromContributor = async (campaignId) => {
    const res = await B.callOk("pledge_to_campaign", {
      campaign_id: campaignId,
      product_variant_id: "pv",
      quantity: 1,
      unit_price: "1.00",
      plan: "Full",
    });
    if (!res.ok) throw new Error(`pledge failed: ${res.error}`);
    const mine = await B.callOk("get_my_contributions", null);
    const contribution = mine.value.find(([, c]) => c.campaign_id === campaignId);
    if (!contribution) throw new Error("contribution not found after pledge");
    return contribution[1].id;
  };

  const contributionStatus = async (contributionId) => {
    const mine = await B.callOk("get_my_contributions", null);
    const found = mine.value.find(([, c]) => c.id === contributionId);
    return found ? found[1] : null;
  };

  // --- A: happy path ------------------------------------------------------
  console.log("\n== A: happy path (create -> pledge -> accept -> pay -> complete) ==");
  await A.callOk("create_enterprise", {
    name: "AB Farm",
    enterprise_type: "farm",
    description: null,
    permalink: "ab-farm",
    visible: true,
  });

  const happy = await newCampaign("AB Harvest", "300.00");
  await activate(happy.id);
  await syncToContributor(happy.address);
  check("contributor sees campaign via shared DHT", true);

  const happyRes = await B.callOk("pledge_to_campaign", {
    campaign_id: happy.id,
    product_variant_id: "pv",
    quantity: 10,
    unit_price: "10.00",
    plan: "Full",
  });
  check("contributor pledges 10 x 10.00", happyRes.ok, String(happyRes.error));
  const happyContribId = await (async () => {
    const mine = await B.callOk("get_my_contributions", null);
    const found = mine.value.find(([, c]) => c.campaign_id === happy.id);
    return found ? found[1].id : null;
  })();

  const accepted = await A.callOk("accept_contribution", {
    campaign_id: happy.id,
    contribution_id: happyContribId,
  });
  check("owner accepts pledge (Pending -> Accepted)", accepted.ok, String(accepted.error));

  const recalc = await A.callOk("recalculate_campaign_totals", happy.id);
  check("owner recalculates totals", recalc.ok, String(recalc.error));
  const happyAfter = await A.callOk("get_campaign", happy.address);
  check(
    "campaign current_amount == 100.00 and fulfilled_quantity == 10",
    happyAfter.ok &&
      happyAfter.value[1].current_amount === "100.00" &&
      happyAfter.value[1].fulfilled_quantity === 10,
    happyAfter.ok
      ? `${happyAfter.value[1].current_amount}/${happyAfter.value[1].fulfilled_quantity}`
      : "n/a",
  );

  const paid = await B.callOk("record_payment", {
    campaign_id: happy.id,
    contribution_id: happyContribId,
    amount: "100.00",
  });
  check("contributor pays in full (Accepted -> Completed)", paid.ok, String(paid.error));
  const doneContribution = await contributionStatus(happyContribId);
  check(
    "contribution Completed with paid_amount 100.00",
    doneContribution &&
      doneContribution.status === "Completed" &&
      doneContribution.paid_amount === "100.00",
    doneContribution ? `${doneContribution.status}/${doneContribution.paid_amount}` : "n/a",
  );

  const fund = await A.callOk("set_campaign_state", {
    campaign_id: happy.id,
    state: "Funded",
  });
  const complete = await A.callOk("set_campaign_state", {
    campaign_id: happy.id,
    state: "Completed",
  });
  check("campaign Funded -> Completed", fund.ok && complete.ok);

  // --- B: decline ---------------------------------------------------------
  console.log("\n== B: decline ==");
  const dec = await newCampaign("Decline Test", "200.00");
  await activate(dec.id);
  await syncToContributor(dec.address);
  const decId = await pledgeFromContributor(dec.id);

  const badAccept = await B.callOk("accept_contribution", {
    campaign_id: dec.id,
    contribution_id: decId,
  });
  check("contributor cannot accept (owner-only)", !badAccept.ok);

  const declined = await A.callOk("decline_contribution", {
    campaign_id: dec.id,
    contribution_id: decId,
  });
  check("owner declines pledge (Pending -> Declined)", declined.ok, String(declined.error));
  const decContribution = await contributionStatus(decId);
  check("contribution is Declined", decContribution && decContribution.status === "Declined");

  // --- B: failure to pay (default) ---------------------------------------
  console.log("\n== B: failure to pay (default) ==");
  const pay = await newCampaign("Default Test", "200.00");
  await activate(pay.id);
  await syncToContributor(pay.address);
  const payRes = await B.callOk("pledge_to_campaign", {
    campaign_id: pay.id,
    product_variant_id: "pv",
    quantity: 2,
    unit_price: "50.00",
    plan: "Installments",
  });
  check("contributor pledges 2 x 50.00 on installments", payRes.ok, String(payRes.error));
  const payId = await (async () => {
    const mine = await B.callOk("get_my_contributions", null);
    const found = mine.value.find(([, c]) => c.campaign_id === pay.id);
    return found ? found[1].id : null;
  })();

  const acceptPay = await A.callOk("accept_contribution", {
    campaign_id: pay.id,
    contribution_id: payId,
  });
  check("owner accepts installment pledge", acceptPay.ok, String(acceptPay.error));

  const partial = await B.callOk("record_payment", {
    campaign_id: pay.id,
    contribution_id: payId,
    amount: "40.00",
  });
  check("partial payment recorded (still Accepted)", partial.ok, String(partial.error));
  const partialContribution = await contributionStatus(payId);
  check(
    "partial payment: paid_amount 40.00, installments_paid 1",
    partialContribution &&
      partialContribution.paid_amount === "40.00" &&
      partialContribution.installments_paid === 1,
    partialContribution
      ? `${partialContribution.paid_amount}/${partialContribution.installments_paid}`
      : "n/a",
  );

  const defaulted = await B.callOk("default_contribution", {
    campaign_id: pay.id,
    contribution_id: payId,
  });
  check("contributor defaults (Accepted -> Defaulted)", defaulted.ok, String(defaulted.error));
  const defContribution = await contributionStatus(payId);
  check("contribution is Defaulted", defContribution && defContribution.status === "Defaulted");

  const reDefault = await B.callOk("default_contribution", {
    campaign_id: pay.id,
    contribution_id: payId,
  });
  check("re-default rejected (terminal state)", !reDefault.ok);

  // --- B: failure to deliver ---------------------------------------------
  console.log("\n== B: failure to deliver ==");
  const fail = await newCampaign("Delivery Test", "200.00");
  await activate(fail.id);

  const batch = await A.callOk("create_batch", {
    campaign_id: fail.id,
    name: "Wheat",
    crop: "Wheat",
    variety: null,
    target_quantity: "100",
    unit: "kg",
    sowing_date: 1_000,
    expected_harvest_at: 2_000,
  });
  check("owner creates batch", batch.ok, String(batch.error));
  const batchId = batch.value;

  const growing = await A.callOk("set_batch_status", {
    campaign_id: fail.id,
    batch_id: batchId,
    status: "Growing",
  });
  check("batch Planned -> Growing", growing.ok, String(growing.error));

  const failedBatch = await A.callOk("set_batch_status", {
    campaign_id: fail.id,
    batch_id: batchId,
    status: "Failed",
  });
  check("batch fails to deliver (Growing -> Failed)", failedBatch.ok, String(failedBatch.error));

  const failedCampaign = await A.callOk("set_campaign_state", {
    campaign_id: fail.id,
    state: "Failed",
  });
  check(
    "campaign Active -> Failed (failure to deliver)",
    failedCampaign.ok,
    String(failedCampaign.error),
  );

  // --- authorization negatives -------------------------------------------
  console.log("\n== authorization / money negatives ==");
  const notOwner = await B.callOk("recalculate_campaign_totals", happy.id);
  check("contributor cannot recalculate totals (owner-only)", !notOwner.ok);

  const zeroPay = await B.callOk("record_payment", {
    campaign_id: happy.id,
    contribution_id: happyContribId,
    amount: "0",
  });
  check("zero payment rejected", !zeroPay.ok);

  const overPay = await B.callOk("record_payment", {
    campaign_id: happy.id,
    contribution_id: happyContribId,
    amount: "10.00",
  });
  check("overpayment on completed contribution rejected", !overPay.ok);

  A.app.client.close();
  B.app.client.close();
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