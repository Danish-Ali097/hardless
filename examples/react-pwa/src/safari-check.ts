/**
 * The Safari check (`safari-check.html`, run by .github/workflows/safari.yml on iPhone, iPad
 * and desktop Safari): prints with the app's own ePOS-Print code, without any clicks, so a
 * simulator can run it. Open it over HTTPS at the computer's LAN address, the way a phone on
 * the same Wi-Fi reaches Hardless, with the Hardless certificate trusted:
 *
 * 1. HTTPS to port 8443 must print (the certificate is trusted and the CORS answers suit
 *    Safari);
 * 2. plain HTTP to port 8080 must be blocked as mixed content.
 *
 * Each receipt carries `?device=` so the workflow can tell from the server's job list which
 * prints arrived. The page shows the results and sets its title to PASS or FAIL.
 */
import { defaultAddress, eposReceipt, printOverNetwork } from "./epos.js";

const device = new URLSearchParams(location.search).get("device") ?? "browser";
const results = document.getElementById("results");
const where = document.getElementById("where");
if (where) where.textContent = `${device}: ${navigator.userAgent}`;

function report(passed: boolean, text: string): void {
  const item = document.createElement("li");
  item.className = passed ? "pass" : "fail";
  item.textContent = `${passed ? "PASS" : "FAIL"}: ${text}`;
  results?.append(item);
}

function receipt(route: string): string {
  return eposReceipt({
    lines: [{ name: `SAFARI ${device} ${route}`, quantity: 1, price: 100 }],
    payment: "card",
  });
}

async function run(): Promise<boolean> {
  if (location.protocol !== "https:") {
    report(false, "open this page over https:// (HTTPS=1 pnpm dev --host); it tests HTTPS pages");
    return false;
  }
  const secure = defaultAddress(location);
  let httpsOk = false;
  try {
    await printOverNetwork(secure, receipt("HTTPS"));
    httpsOk = true;
    report(true, `printed over HTTPS (${secure})`);
  } catch (error) {
    report(false, `HTTPS print failed: ${error instanceof Error ? error.message : String(error)}`);
  }

  const insecure = defaultAddress({ protocol: "http:", hostname: location.hostname });
  let blocked = false;
  try {
    await printOverNetwork(insecure, receipt("HTTP"));
    report(false, `plain HTTP was not blocked (${insecure})`);
  } catch {
    blocked = true;
    report(true, "plain HTTP from this HTTPS page was blocked (mixed content)");
  }
  return httpsOk && blocked;
}

void run().then((passed) => {
  document.title = `Safari check: ${passed ? "PASS" : "FAIL"}`;
});
