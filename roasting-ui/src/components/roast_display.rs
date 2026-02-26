use leptos::prelude::*;
use roasting_app::domain::Roast;

fn simple_markdown_to_html(text: &str) -> String {
    let mut result = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let processed = line
            .replace("**", "<strong>")
            .replace("__", "<strong>");

        let processed = fix_strong_tags(&processed);

        let processed = processed
            .replace("*", "<em>")
            .replace("_", "<em>");

        let processed = fix_em_tags(&processed);

        if line.starts_with("# ") {
            result.push_str(&format!("<h3>{}</h3>", &processed[2..]));
        } else if line.starts_with("## ") {
            result.push_str(&format!("<h4>{}</h4>", &processed[3..]));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            result.push_str(&format!("<li>{}</li>", &processed[2..]));
        } else {
            result.push_str(&format!("<p>{}</p>", processed));
        }
    }

    result
}

fn fix_strong_tags(text: &str) -> String {
    let count = text.matches("<strong>").count();
    let mut result = text.to_string();
    for i in 0..count {
        if i % 2 == 1 {
            result = result.replacen("<strong>", "</strong>", 1);
        }
    }
    result
}

fn fix_em_tags(text: &str) -> String {
    let count = text.matches("<em>").count();
    let mut result = text.to_string();
    for i in 0..count {
        if i % 2 == 1 {
            result = result.replacen("<em>", "</em>", 1);
        }
    }
    result
}

#[component]
pub fn RoastDisplay(roast: Roast) -> impl IntoView {
    let html_content = simple_markdown_to_html(&roast.roast_text);

    view! {
        <div class="roast">
            <h2 class="roast__title">
                "Roasting: " {roast.startup_name}
            </h2>
            <div class="roast__content" inner_html=html_content>
            </div>
            <div class="roast__actions">
                <a href="/" class="roast__button roast__button--primary">
                    "Roast Lagi!"
                </a>
            </div>
        </div>
    }
}
