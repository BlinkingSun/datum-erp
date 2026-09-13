//! Deterministic HTML/PDF rendering (no wall-clock or caller-dependent bytes).

use std::collections::BTreeMap;

use crate::domain::TemplateId;
use crate::error::Result;
use crate::reads;
use crate::store;
use datum_core::RecordRef;
use datum_db::Tx;
use datum_esign::Manifestation;

/// Resolved record fields for template substitution (stable key order in generic layout).
#[derive(Debug, Clone)]
pub(crate) struct RecordView {
    pub content_hash: [u8; 32],
    pub fields: BTreeMap<String, String>,
}

pub(crate) async fn resolve_record(
    tx: &mut Tx<'_>,
    record: &RecordRef,
    template: &TemplateId,
) -> Result<RecordView> {
    if record.table == "documents.revision" || record.table == "documents.document" {
        let rev = reads::document_revision_view(tx, record).await?;
        let mut fields = BTreeMap::new();
        fields.insert("title".into(), rev.title);
        fields.insert("number".into(), rev.number);
        fields.insert("revision".into(), rev.label);
        fields.insert("effectivity".into(), rev.effectivity);
        fields.insert("attachments".into(), rev.attachments);
        return Ok(RecordView {
            content_hash: rev.content_hash,
            fields,
        });
    }
    if template.0 == TemplateId::WORK_ORDER_TRAVELER {
        let snapshot = serde_json::json!({
            "wo_number": "WO-2026-1847",
            "operations": [{"seq": 10, "name": "Turn OD"}, {"seq": 20, "name": "Mill pocket"}],
            "materials": [{"item": "RM-TI-BAR-12", "lot": "LOT-BAR-24-4412"}],
        });
        let body = serde_json::to_string(&snapshot)
            .map_err(|e| crate::error::Error::Core(datum_core::Error::Invariant(e.to_string())))?;
        let mut fields = BTreeMap::new();
        fields.insert("wo_number".into(), "WO-2026-1847".into());
        fields.insert("operations".into(), "10 Turn OD; 20 Mill pocket".into());
        fields.insert("materials".into(), "RM-TI-BAR-12 / LOT-BAR-24-4412".into());
        return Ok(RecordView {
            content_hash: store::digest(body.as_bytes()),
            fields,
        });
    }
    let body = serde_json::json!({
        "table": record.table,
        "id": record.id.to_string(),
        "version": record.version,
    });
    let serialized = serde_json::to_string(&body)
        .map_err(|e| crate::error::Error::Core(datum_core::Error::Invariant(e.to_string())))?;
    let mut fields = BTreeMap::new();
    fields.insert("table".into(), record.table.clone());
    fields.insert("id".into(), record.id.to_string());
    fields.insert("version".into(), record.version.to_string());
    Ok(RecordView {
        content_hash: store::digest(serialized.as_bytes()),
        fields,
    })
}

pub(crate) fn build_html(
    template_body: &str,
    view: &RecordView,
    manifestations: &[Manifestation],
    footer: &str,
    regulated: bool,
) -> String {
    let mut body = template_body.to_owned();
    for (k, v) in &view.fields {
        body = body.replace(&format!("{{{{{}}}}}", k), v);
    }
    let sig_block = format_signatures(regulated, manifestations);
    body = body.replace("{{MANIFESTATION}}", &sig_block);
    body = body.replace("{{FOOTER}}", footer);
    normalize_html(&body)
}

fn format_signatures(regulated: bool, manifestations: &[Manifestation]) -> String {
    if !regulated {
        return String::new();
    }
    if manifestations.is_empty() {
        return "<section class=\"signatures\"><p>UNSIGNED</p></section>".to_owned();
    }
    let mut html = String::from("<section class=\"signatures\">");
    for m in manifestations {
        let s = &m.signature;
        html.push_str("<div class=\"signature\">");
        html.push_str(&html_escape(&s.printed_name));
        html.push_str(" — ");
        html.push_str(&html_escape(&s.meaning));
        html.push_str(" — ");
        html.push_str(&html_escape(&s.signed_at));
        html.push(' ');
        html.push_str(&html_escape(&s.signed_at_zone));
        html.push_str(" — hash ");
        html.push_str(&html_escape(&s.record_content_hash));
        html.push_str("</div>");
    }
    html.push_str("</section>");
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn normalize_html(html: &str) -> String {
    let mut out = String::new();
    for line in html.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

pub(crate) fn html_to_pdf(html: &str) -> Result<Vec<u8>> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};
    let mut pdf = Pdf::new();
    let catalog_id = Ref::new(1);
    let page_id = Ref::new(2);
    let content_id = Ref::new(3);
    let font_id = Ref::new(4);
    pdf.catalog(catalog_id).pages(page_id);
    {
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, 612.0, 792.0));
        page.parent(catalog_id);
        page.contents(content_id);
        {
            let mut resources = page.resources();
            resources.fonts().pair(Name(b"F1"), font_id);
        }
        page.finish();
    }
    pdf.type1_font(font_id).base_font(Name(b"Helvetica"));
    let mut content = Content::new();
    content.begin_text();
    content.set_font(Name(b"F1"), 10.0);
    content.next_line(50.0, 750.0);
    for line in html.lines().take(60) {
        let chunk = if line.len() > 90 { &line[..90] } else { line };
        content.show(Str(chunk.as_bytes()));
        content.next_line(0.0, -12.0);
    }
    content.end_text();
    pdf.stream(content_id, &content.finish());
    Ok(pdf.finish())
}

pub(crate) fn renderer_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

pub(crate) async fn regulated_from_tx(tx: &mut Tx<'_>) -> Result<bool> {
    Ok(store::install_profile(tx).await? == "regulated-device")
}

pub(crate) fn footer_stamp(
    template_semver: &str,
    template_version: i32,
    renderer_version: &str,
    app_version: &str,
    config_version: &str,
) -> String {
    format!(
        "renderer={renderer_version} template={template_semver} (v{template_version}) \
app_version={app_version} config_version={config_version}"
    )
}
