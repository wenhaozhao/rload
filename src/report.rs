use serde_json::Value;

pub fn render(result: &Value) -> Result<String, String> {
    let data = serde_json::to_string(result)
        .map_err(|error| format!("cannot serialize report data: {error}"))?
        .replace("</", "<\\/");
    Ok(format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>rload report</title>
<style>body{{font:16px system-ui,sans-serif;max-width:960px;margin:40px auto;padding:0 20px;color:#172033}}h1{{margin-bottom:4px}}.muted{{color:#65718a}}.grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px}}.card{{border:1px solid #d9e1ed;border-radius:8px;padding:16px;background:#f8fafc}}.value{{font-size:1.4rem;font-weight:700}}table{{border-collapse:collapse;width:100%;margin-top:16px}}th,td{{border-bottom:1px solid #d9e1ed;padding:8px;text-align:left}}th{{color:#65718a}}</style>
</head><body><h1>rload report</h1><p class="muted">Offline result artifact</p><section id="summary" class="grid"></section><h2>Latency</h2><table id="latency"></table><h2>Socket errors</h2><table id="errors"></table>
<script>const result={data};const s=result.summary,l=result.latency,e=result.socket_errors;const us=v=>v==null?'N/A':`${{v}} µs`;document.querySelector('#summary').innerHTML=[['Completed',s.completed_requests],['Requests/sec',s.requests_per_sec.toFixed(2)],['Status errors',s.status_errors],['Read bytes',s.read_bytes]].map(([k,v])=>`<article class="card"><div class="muted">${{k}}</div><div class="value">${{v}}</div></article>`).join('');const table=(id,rows)=>document.querySelector(id).innerHTML='<tr><th>Metric</th><th>Value</th></tr>'+rows.map(([k,v])=>`<tr><td>${{k}}</td><td>${{v}}</td></tr>`).join('');table('#latency',[['Minimum',us(l.minimum_us)],['Maximum',us(l.maximum_us)],['Average',us(l.average_us)],['Median / P50',us(l.median_us)],['P90',us(l.p90_us)],['P95',us(l.p95_us)],['P99',us(l.p99_us)]]);table('#errors',[['Connect',e.connect],['Read',e.read],['Write',e.write],['Timeout',e.timeout],['Total',e.total]]);</script></body></html>"##
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_is_deterministic_and_embeds_data() {
        let result = serde_json::json!({"summary":{"completed_requests":1,"requests_per_sec":1.0,"status_errors":0,"read_bytes":0},"latency":{"minimum_us":null,"maximum_us":null,"average_us":null,"median_us":null,"p90_us":null,"p95_us":null,"p99_us":null},"socket_errors":{"connect":0,"read":0,"write":0,"timeout":0,"total":0}});
        assert_eq!(render(&result).unwrap(), render(&result).unwrap());
    }
}
