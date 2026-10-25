use super::{
    Paper,
    ascii::{duration, number},
    logo, model_name, safe_text, usage_rows,
};
use crate::{
    pricing::{Pricing, money},
    receipt::Receipt,
};

fn escape(text: &str) -> String {
    safe_text(text)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render(receipt: &Receipt, pricing: &Pricing, paper: Paper) -> String {
    let width = paper.millimeters();
    let compact = matches!(paper, Paper::Mm58);
    let screen_width = if compact { 300 } else { 380 };
    let print_font = if compact { "9.5" } else { "11.5" };
    let mark_width = if compact { 18 } else { 24 };
    let mark_height = mark_width / 2;
    let mark = logo::svg();
    let project = escape(&receipt.project);
    let session = escape(&receipt.session_id);
    let date = receipt.started_at.format("%Y-%m-%d %H:%M:%S UTC");
    let elapsed = duration(receipt.ended_at - receipt.started_at);
    let active = receipt
        .active_duration
        .map(duration)
        .unwrap_or_else(|| "N/A".into());
    let total = money(pricing.total(receipt));
    let total_size = if total.len() > 12 { 14 } else { 24 };
    let total_print_size = if total.len() > 12 {
        if compact { 11 } else { 16 }
    } else {
        20
    };
    let source = escape(&pricing.source);
    let basis = escape(&pricing.basis);
    let version = escape(&pricing.version);
    let mut items = String::new();
    for usage in &receipt.models {
        let name = escape(&model_name(&usage.model));
        let model_id = escape(&usage.model);
        let cost = money(pricing.model_cost(usage));
        items.push_str(&format!(
            r#"<section class="item" aria-label="{name}">
<h2><span title="{model_id}">{name}</span><span class="amount">{cost}</span></h2><dl>"#
        ));
        for (label, count) in usage_rows(usage.tokens) {
            items.push_str(&format!(
                "<div><dt>{label}</dt><dd>{}</dd></div>",
                number(count)
            ));
        }
        items.push_str("</dl></section>\n");
    }
    if receipt.total.is_none() {
        items.push_str("<p class=\"empty\">No token usage recorded.</p>");
    }
    let mut notes = String::new();
    if pricing.total(receipt).is_none() && receipt.total.is_some() {
        notes.push_str("<p>N/A: unknown model or unpriced usage.</p>");
    }
    for warning in &receipt.warnings {
        notes.push_str(&format!("<p>{}</p>", escape(warning)));
    }
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<meta name="color-scheme" content="light">
<title>Codex receipt — {project}</title>
<style>
:root {{
  --paper: #faf7ef;
  --ink: #26251f;
  --muted: #77746b;
  --rule: #77746b;
  --screen-width: {screen_width}px;
  --print-width: {width}mm;
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  min-height: 100vh;
  padding: 64px 24px 80px;
  background: #272a27;
  background-image: repeating-linear-gradient(0deg, #ffffff02 0 1px, transparent 1px 3px);
  color: var(--ink);
  font-family: 'Courier New', Courier, monospace;
  -webkit-font-smoothing: auto;
}}
.receipt {{
  position: relative;
  width: var(--screen-width);
  max-width: 100%;
  margin: 0 auto;
  padding: 44px 24px 32px;
  background: var(--paper);
  box-shadow: 0 2px 3px #0003, 0 24px 64px #0005;
  font-size: 13px;
  line-height: 1.55;
  letter-spacing: -.35px;
  font-variant-numeric: tabular-nums;
}}
.receipt::after {{
  content: '';
  position: absolute;
  left: 0; right: 0; bottom: -8px;
  height: 8px;
  background: linear-gradient(135deg, var(--paper) 50%, transparent 50%) 0 0/12px 12px repeat-x,
              linear-gradient(225deg, var(--paper) 50%, transparent 50%) 0 0/12px 12px repeat-x;
}}
header {{ text-align: center; }}
.mark {{ display: block; width: 100px; height: 50px; margin: 0 auto 20px; fill: currentColor; shape-rendering: crispEdges; }}
h1 {{ margin: 0; font-size: 10px; line-height: 1.6; letter-spacing: 3px; font-weight: 700; }}
.identity {{ margin: 27px 0 30px; font-size: 11px; line-height: 1.7; letter-spacing: -.3px; }}
.identity p {{ margin: 0; overflow-wrap: anywhere; }}
.identity .session-label {{ margin-top: 7px; font-size: 9px; letter-spacing: 1.5px; color: var(--muted); }}
.identity .session {{ font-size: 10px; }}
.identity time {{ display: block; margin-top: 3px; font-size: 10px; }}
.item {{ margin: 0 0 24px; break-inside: avoid; }}
.item h2 {{
  display: flex; justify-content: space-between; align-items: baseline; gap: 12px;
  margin: 0; padding: 8px 0 7px;
  border-top: 3px double var(--rule); border-bottom: 1px dashed var(--rule);
  font-size: 14px; line-height: 1.4; font-weight: 700;
}}
.item h2 > :first-child {{ min-width: 0; overflow-wrap: anywhere; }}
.amount {{ flex-shrink: 0; white-space: nowrap; font-variant-numeric: tabular-nums; }}
dl {{ margin: 10px 0 0; }}
dl > div {{ display: flex; justify-content: space-between; gap: 10px; }}
dt {{ padding-left: 10px; min-width: 0; overflow-wrap: anywhere; }}
dd {{ margin: 0; text-align: right; white-space: nowrap; flex-shrink: 0; }}
.total {{ border-top: 3px double var(--rule); border-bottom: 3px double var(--rule); padding: 10px 0 11px; break-inside: avoid; }}
.total .caption {{ margin: 0 0 4px; font-size: 9px; letter-spacing: 1.2px; color: var(--muted); }}
.total .sum {{ display: flex; justify-content: space-between; gap: 12px; font-size: {total_size}px; line-height: 1.1; font-weight: 700; letter-spacing: -.8px; }}
footer {{ break-inside: avoid; }}
.meta {{ margin-top: 23px; font-size: 11px; }}
.meta dt {{ padding: 0; }}
.cashier {{ margin: 12px 0 0; font-size: 11px; }}
.thanks {{ margin: 30px 0 25px; text-align: center; font-size: 14px; font-weight: 400; letter-spacing: -.45px; }}
.fine-print {{ color: var(--muted); text-align: center; font-size: 9px; line-height: 1.65; letter-spacing: -.1px; }}
.fine-print p {{ margin: 0; }}
.fine-print a {{ color: inherit; text-decoration-thickness: .5px; text-underline-offset: 2px; }}
.fine-print .pricing {{ margin-top: 8px; }}
.notes {{ margin-top: 12px; font-size: 10px; line-height: 1.5; overflow-wrap: anywhere; }}
.notes:empty {{ display: none; }}
.notes p {{ margin: 4px 0 0; }}
.empty {{ margin: 20px 0; text-align: center; }}
@page {{ margin: 0; }}
@media (max-width: 380px) {{
  body {{ padding: 28px 12px 44px; }}
  .receipt {{ padding-left: 18px; padding-right: 18px; font-size: 12px; }}
  .item h2 {{ font-size: 13px; }}
}}
@media print {{
  :root {{ --paper: #fff; --ink: #000; --muted: #444; --rule: #000; }}
  body {{ min-height: 0; background: none; padding: 0; }}
  .receipt {{ width: var(--print-width); max-width: none; margin: 0; padding: 7mm 4mm 5mm; box-shadow: none; font-size: {print_font}px; letter-spacing: -.3px; }}
  .receipt::after {{ display: none; }}
  .mark {{ width: {mark_width}mm; height: {mark_height}mm; margin-bottom: 4mm; }}
  h1 {{ font-size: 8px; letter-spacing: 2px; }}
  .identity {{ margin: 5mm 0 6mm; font-size: 9px; }}
  .identity .session-label {{ font-size: 8px; }}
  .identity .session {{ font-size: 8px; }}
  .identity time {{ font-size: 8.5px; }}
  .item {{ margin-bottom: 5mm; }}
  .item h2 {{ padding: 2mm 0 1.5mm; font-size: {print_font}px; gap: 2mm; }}
  dl {{ margin-top: 2mm; }}
  dt {{ padding-left: 2mm; }}
  .total {{ padding: 2.5mm 0; }}
  .total .caption {{ font-size: 7.5px; }}
  .total .sum {{ font-size: {total_print_size}px; }}
  .meta, .cashier {{ font-size: 9px; }}
  .meta {{ margin-top: 4mm; }}
  .thanks {{ margin: 6mm 0 5mm; font-size: 12px; }}
  .fine-print {{ font-size: 8px; }}
  .notes {{ font-size: 8px; }}
}}
</style>
</head>
<body>
<main class="receipt" aria-label="Codex session receipt">
<header>
{mark}
<h1>CODEX / RECEIPTS</h1>
<div class="identity">
<p>Project: {project}</p>
<p class="session-label">SESSION RECEIPT</p>
<p class="session">{session}</p>
<time>{date}</time>
</div>
</header>
{items}
<section class="total" aria-label="Estimated API cost">
<p class="caption">ESTIMATED API COST</p>
<div class="sum"><span>TOTAL</span><span class="amount">{total}</span></div>
</section>
<footer>
<dl class="meta"><div><dt>Elapsed</dt><dd>{elapsed}</dd></div><div><dt>Active turns</dt><dd>{active}</dd></div></dl>
<p class="cashier">CASHIER: Codex</p>
<p class="thanks">Thank you for building!</p>
<div class="fine-print" title="{basis}">
<p>Standard short-context API estimate.</p>
<p>Excludes premiums, tools &amp; extras.</p>
<p>Not a subscription charge.</p>
<p class="pricing"><a href="{source}">OpenAI pricing</a> / {version}</p>
</div>
<div class="notes">{notes}</div>
</footer>
</main>
</body>
</html>
"#
    )
}
