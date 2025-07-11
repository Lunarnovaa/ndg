use std::{collections::HashMap, fmt::Write, fs, path::Path};

use anyhow::{Context, Result};
use tera::Tera;

use crate::{
    config::Config,
    formatter::{markdown::Header, options::NixOption},
};

// Template constants - these serve as fallbacks
const DEFAULT_TEMPLATE: &str = include_str!("../../templates/default.html");
const OPTIONS_TEMPLATE: &str = include_str!("../../templates/options.html");
const SEARCH_TEMPLATE: &str = include_str!("../../templates/search.html");
const OPTIONS_TOC_TEMPLATE: &str = include_str!("../../templates/options_toc.html");

/// Render a documentation page
pub fn render(
    config: &Config,
    content: &str,
    title: &str,
    headers: &[Header],
    _rel_path: &Path,
) -> Result<String> {
    let mut tera = Tera::default();
    let template_content = get_template_content(config, "default.html", DEFAULT_TEMPLATE)?;
    tera.add_raw_template("default", &template_content)?;

    // Generate table of contents from headers
    let toc = generate_toc(headers);

    // Generate document navigation
    let doc_nav = generate_doc_nav(config);

    // Check if options are available
    let has_options = if config.module_options.is_some() {
        ""
    } else {
        "style=\"display:none;\""
    };

    // Generate custom scripts HTML
    let custom_scripts = generate_custom_scripts(config)?;

    // Create context
    let mut tera_context = tera::Context::new();
    tera_context.insert("content", content);
    tera_context.insert("title", title);
    tera_context.insert("site_title", &config.title);
    tera_context.insert("footer_text", &config.footer_text);
    tera_context.insert("toc", &toc);
    tera_context.insert("doc_nav", &doc_nav);
    tera_context.insert("has_options", has_options);
    tera_context.insert("custom_scripts", &custom_scripts);
    tera_context.insert("generate_search", &config.generate_search);

    // Render the template
    let html = tera.render("default", &tera_context)?;
    Ok(html)
}

/// Render `NixOS` module options page
pub fn render_options(config: &Config, options: &HashMap<String, NixOption>) -> Result<String> {
    let mut tera = Tera::default();
    let options_template = get_template_content(config, "options.html", OPTIONS_TEMPLATE)?;
    tera.add_raw_template("options", &options_template)?;

    // Create options HTML
    let options_html = generate_options_html(options);

    // Load the options_toc template from template directory or use default
    let options_toc_template =
        get_template_content(config, "options_toc.html", OPTIONS_TOC_TEMPLATE)?;
    tera.add_raw_template("options_toc", &options_toc_template)?;

    // Generate options TOC using Tera templating
    let options_toc = generate_options_toc(options, config, &tera)?;

    // Generate document navigation
    let doc_nav = generate_doc_nav(config);

    // Generate custom scripts HTML
    let custom_scripts = generate_custom_scripts(config)?;

    // Create context
    let mut tera_context = tera::Context::new();
    tera_context.insert("title", &format!("{} - Options", config.title));
    tera_context.insert("site_title", &config.title);
    tera_context.insert("heading", &format!("{} Options", config.title));
    tera_context.insert("options", &options_html);
    tera_context.insert("footer_text", &config.footer_text);
    tera_context.insert("custom_scripts", &custom_scripts);
    tera_context.insert("doc_nav", &doc_nav);
    tera_context.insert("has_options", "class=\"active\"");
    tera_context.insert("toc", &options_toc);
    tera_context.insert("generate_search", &config.generate_search);

    // Render the template
    let html = tera.render("options", &tera_context)?;
    Ok(html)
}

/// Generate specialized TOC for options page
fn generate_options_toc(
    options: &HashMap<String, NixOption>,
    config: &Config,
    tera: &Tera,
) -> Result<String> {
    // Configured depth or default of 2
    let depth = config.options_toc_depth;

    let mut grouped_options: HashMap<String, Vec<&NixOption>> = HashMap::new();
    let mut direct_parent_options: HashMap<String, &NixOption> = HashMap::new();

    for option in options.values() {
        let parent = get_option_parent(&option.name, depth);

        // Check if this option exactly matches its parent category
        if option.name == parent {
            direct_parent_options.insert(parent.clone(), option);
        }

        // Add to grouped options
        grouped_options.entry(parent).or_default().push(option);
    }

    // Separate categories into single options and dropdown categories
    let mut single_options: Vec<tera::Value> = Vec::new();
    let mut dropdown_categories: Vec<tera::Value> = Vec::new();

    for (parent, opts) in &grouped_options {
        let has_multiple_options = opts.len() > 1;
        let has_child_options =
            opts.len() > usize::from(direct_parent_options.contains_key(parent));

        if !has_multiple_options && !has_child_options {
            // Single option with no children
            let option = opts[0];
            let option_value = tera::to_value({
                let mut map = tera::Map::new();
                map.insert("name".to_string(), tera::to_value(&option.name)?);
                map.insert("internal".to_string(), tera::to_value(option.internal)?);
                map.insert("read_only".to_string(), tera::to_value(option.read_only)?);
                map
            })?;
            single_options.push(option_value);
        } else {
            // Category with multiple options or child options
            let mut category = tera::Map::new();
            category.insert("name".to_string(), tera::to_value(parent)?);
            category.insert("count".to_string(), tera::to_value(opts.len())?);

            // Add parent option if it exists
            if let Some(parent_option) = direct_parent_options.get(parent) {
                let parent_option_value = tera::to_value({
                    let mut map = tera::Map::new();
                    map.insert("name".to_string(), tera::to_value(&parent_option.name)?);
                    map.insert(
                        "internal".to_string(),
                        tera::to_value(parent_option.internal)?,
                    );
                    map.insert(
                        "read_only".to_string(),
                        tera::to_value(parent_option.read_only)?,
                    );
                    map
                })?;
                category.insert("parent_option".to_string(), parent_option_value);
            }

            // Add child options
            let mut children = Vec::new();
            let mut child_options: Vec<&NixOption> = opts
                .iter()
                .filter(|opt| opt.name != *parent)
                .copied()
                .collect();

            // Sort by suffix
            child_options.sort_by(|a, b| {
                let a_suffix = a
                    .name
                    .strip_prefix(&format!("{parent}."))
                    .unwrap_or(&a.name);
                let b_suffix = b
                    .name
                    .strip_prefix(&format!("{parent}."))
                    .unwrap_or(&b.name);
                a_suffix.cmp(b_suffix)
            });

            for option in child_options {
                let display_name = option
                    .name
                    .strip_prefix(&format!("{parent}."))
                    .unwrap_or(&option.name);

                let child_value = tera::to_value({
                    let mut map = tera::Map::new();
                    map.insert("name".to_string(), tera::to_value(&option.name)?);
                    map.insert("display_name".to_string(), tera::to_value(display_name)?);
                    map.insert("internal".to_string(), tera::to_value(option.internal)?);
                    map.insert("read_only".to_string(), tera::to_value(option.read_only)?);
                    map
                })?;
                children.push(child_value);
            }

            category.insert("children".to_string(), tera::to_value(children)?);
            dropdown_categories.push(tera::to_value(category)?);
        }
    }

    // Sort single options alphabetically
    single_options.sort_by(|a, b| {
        let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        a_name.cmp(b_name)
    });

    // Sort dropdown categories
    dropdown_categories.sort_by(|a, b| {
        let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");

        let a_components = a_name.split('.').count();
        let b_components = b_name.split('.').count();

        // Sort by component count first
        match a_components.cmp(&b_components) {
            std::cmp::Ordering::Equal => a_name.cmp(b_name), // Then alphabetically
            other => other,
        }
    });

    let mut tera_context = tera::Context::new();
    tera_context.insert("single_options", &single_options);
    tera_context.insert("dropdown_categories", &dropdown_categories);

    // Render the template
    let rendered = tera.render("options_toc", &tera_context)?;

    Ok(rendered)
}

/// Extract the parent category from an option name with configurable depth
fn get_option_parent(option_name: &str, depth: usize) -> String {
    let parts: Vec<&str> = option_name.split('.').collect();
    if parts.len() <= depth {
        option_name.to_string()
    } else {
        parts[..depth].join(".")
    }
}

/// Render search page
pub fn render_search(config: &Config, context: &HashMap<&str, String>) -> Result<String> {
    // Skip rendering if search is disabled
    if !config.generate_search {
        return Err(anyhow::anyhow!("Search functionality is disabled"));
    }

    let mut tera = Tera::default();
    let search_template = get_template_content(config, "search.html", SEARCH_TEMPLATE)?;
    tera.add_raw_template("search", &search_template)?;

    let title_str = context
        .get("title")
        .cloned()
        .unwrap_or_else(|| format!("{} - Search", config.title));

    // Generate document navigation
    let doc_nav = generate_doc_nav(config);

    // Generate custom scripts HTML
    let custom_scripts = generate_custom_scripts(config)?;

    // Check if options are available
    let has_options = if config.module_options.is_some() {
        ""
    } else {
        "style=\"display:none;\""
    };

    // Create Tera context
    let mut tera_context = tera::Context::new();
    tera_context.insert("title", &title_str);
    tera_context.insert("site_title", &config.title);
    tera_context.insert("heading", "Search");
    tera_context.insert("footer_text", &config.footer_text);
    tera_context.insert("custom_scripts", &custom_scripts);
    tera_context.insert("doc_nav", &doc_nav);
    tera_context.insert("has_options", has_options);
    tera_context.insert("toc", ""); // No TOC for search page
    tera_context.insert("generate_search", &true); // Always true for search page

    // Render the template
    let html = tera.render("search", &tera_context)?;
    Ok(html)
}

/// Get the template content from file in template directory or use default
fn get_template_content(config: &Config, template_name: &str, fallback: &str) -> Result<String> {
    // Try to get the template from the configured template directory
    if let Some(template_dir) = config.get_template_path() {
        let template_path = template_dir.join(template_name);
        if template_path.exists() {
            return fs::read_to_string(&template_path).with_context(|| {
                format!(
                    "Failed to read custom template file: {}. Check file permissions and ensure the file is valid UTF-8",
                    template_path.display()
                )
            });
        }
    }

    // If template_path is specified but doesn't point to a directory with our
    // template
    if let Some(template_path) = &config.template_path {
        if template_path.exists() && template_name == "default.html" {
            // XXX: For backward compatibility
            // If template_path is a file, use it for default.html
            return fs::read_to_string(template_path).with_context(|| {
                format!(
                    "Failed to read custom template file: {}. Check file permissions and ensure the file is valid UTF-8",
                    template_path.display()
                )
            });
        }
    }

    // Use fallback embedded template if no custom template found
    Ok(fallback.to_string())
}

/// Generate the document navigation HTML
fn generate_doc_nav(config: &Config) -> String {
    let mut doc_nav = String::new();

    // Define anchor pattern regex
    let anchor_pattern = regex::Regex::new(r"\s*\{#[a-zA-Z0-9_-]+\}\s*$").unwrap();

    // Only process markdown files if input_dir is provided
    if let Some(input_dir) = &config.input_dir {
        let entries: Vec<_> = walkdir::WalkDir::new(input_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();

        if !entries.is_empty() {
            for entry in entries {
                let path = entry.path();
                if let Ok(rel_doc_path) = path.strip_prefix(input_dir) {
                    let mut html_path = rel_doc_path.to_path_buf();
                    html_path.set_extension("html");

                    let page_title = fs::read_to_string(path).map_or_else(
                        |_| {
                            html_path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        },
                        |content| {
                            content.lines().next().map_or_else(
                                || {
                                    html_path
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string()
                                },
                                |first_line| {
                                    first_line.strip_prefix("# ").map_or_else(
                                        || {
                                            html_path
                                                .file_stem()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string()
                                        },
                                        |title| {
                                            // Clean the title of any inline anchors
                                            let clean_title = anchor_pattern
                                                .replace_all(title.trim(), "")
                                                .to_string();
                                            clean_title
                                        },
                                    )
                                },
                            )
                        },
                    );

                    writeln!(
                        doc_nav,
                        "<li><a href=\"{}\">{}</a></li>",
                        html_path.to_string_lossy(),
                        page_title
                    )
                    .unwrap();
                }
            }
        }
    }

    // Add link to options page if module_options is configured
    if doc_nav.is_empty() && config.module_options.is_some() {
        doc_nav.push_str("<li><a href=\"options.html\">Module Options</a></li>\n");
    }

    // Add search link only if search is enabled
    if config.generate_search {
        doc_nav.push_str("<li><a href=\"search.html\">Search</a></li>\n");
    }

    doc_nav
}

/// Generate custom scripts HTML
fn generate_custom_scripts(config: &Config) -> Result<String> {
    let mut custom_scripts = String::new();

    // Add any user scripts from script_paths. This is additive, not replacing. To replace
    // default content, the user should specify `--template-dir` or `--template` instead.
    for script_path in &config.script_paths {
        write!(
            custom_scripts,
            "<script defer src=\"{}\"></script>",
            script_path.to_string_lossy()
        )?;
    }

    Ok(custom_scripts)
}

/// Generate table of contents from headers
fn generate_toc(headers: &[Header]) -> String {
    let mut toc = String::new();
    let mut current_level = 0;

    for header in headers {
        // Only include h1, h2, h3 in TOC
        if header.level <= 3 {
            // Adjust TOC nesting
            while current_level < header.level - 1 {
                toc.push_str("<ul>");
                current_level += 1;
            }
            while current_level > header.level - 1 {
                toc.push_str("</ul>");
                current_level -= 1;
            }

            // Add TOC item
            if current_level == header.level - 1 {
                toc.push_str("<li>");
            } else {
                toc.push_str("</li><li>");
            }

            writeln!(toc, "<a href=\"#{}\">{}</a>", header.id, header.text).unwrap();
        }
    }

    // Close open tags
    while current_level > 0 {
        toc.push_str("</li></ul>");
        current_level -= 1;
    }
    if !headers.is_empty() && !toc.is_empty() && !toc.ends_with("</li>") {
        toc.push_str("</li>");
    }

    toc
}

/// Generate the options HTML content
fn generate_options_html(options: &HashMap<String, NixOption>) -> String {
    let mut options_html = String::with_capacity(options.len() * 500); // FIXME: Rough estimate for capacity

    // Sort options by name
    let mut option_keys: Vec<_> = options.keys().collect();
    option_keys.sort();

    for key in option_keys {
        let option = &options[key];
        let option_id = format!("option-{}", option.name.replace('.', "-"));

        // Open option container with ID for direct linking
        writeln!(options_html, "<div class=\"option\" id=\"{option_id}\">").unwrap();

        // Option name with anchor link and copy button
        write!(
            options_html,
            "  <h3 class=\"option-name\">\n    <a href=\"#{}\" class=\"option-anchor\">{}</a>\n    <span \
             class=\"copy-link\" title=\"Copy link to this option\"></span>\n    <span \
             class=\"copy-feedback\">Link copied!</span>\n  </h3>\n",
            option_id, option.name
        )
        .unwrap();

        // Option metadata (internal/readOnly)
        let mut metadata = Vec::new();
        if option.internal {
            metadata.push("internal");
        }
        if option.read_only {
            metadata.push("read-only");
        }

        if !metadata.is_empty() {
            writeln!(
                options_html,
                "  <div class=\"option-metadata\">{}</div>",
                metadata.join(", ")
            )
            .unwrap();
        }

        // Option type
        writeln!(
            options_html,
            "  <div class=\"option-type\">Type: <code>{}</code></div>",
            option.type_name
        )
        .unwrap();

        // Option description
        writeln!(
            options_html,
            "  <div class=\"option-description\">{}</div>",
            option.description
        )
        .unwrap();

        // Add default value if available
        add_default_value(&mut options_html, option);

        // Add example if available
        add_example_value(&mut options_html, option);

        // Option declared in - now with hyperlink support
        if let Some(declared_in) = &option.declared_in {
            if let Some(url) = &option.declared_in_url {
                writeln!(
                    options_html,
                    "  <div class=\"option-declared\">Declared in: <code><a href=\"{url}\" \
                     target=\"_blank\">{declared_in}</a></code></div>"
                )
                .unwrap();
            } else {
                writeln!(
                    options_html,
                    "  <div class=\"option-declared\">Declared in: <code>{declared_in}</code></div>"
                )
                .unwrap();
            }
        }

        // Close option div
        options_html.push_str("</div>\n");
    }

    options_html
}

/// Add default value to options HTML
fn add_default_value(html: &mut String, option: &NixOption) {
    if let Some(default_text) = &option.default_text {
        // Remove surrounding backticks if present (from literalExpression)
        let clean_default = if default_text.starts_with('`')
            && default_text.ends_with('`')
            && default_text.len() > 2
        {
            &default_text[1..default_text.len() - 1]
        } else {
            default_text
        };

        writeln!(
            html,
            "  <div class=\"option-default\">Default: <code>{clean_default}</code></div>"
        )
        .unwrap();
    } else if let Some(default_val) = &option.default {
        writeln!(
            html,
            "  <div class=\"option-default\">Default: <code>{default_val}</code></div>"
        )
        .unwrap();
    }
}

/// Add example value to options HTML
fn add_example_value(html: &mut String, option: &NixOption) {
    if let Some(example_text) = &option.example_text {
        // Process the example text to preserve code formatting
        if example_text.contains('\n') {
            // Multi-line examples - preserve formatting with pre/code
            // Process special characters to ensure valid HTML
            let safe_example = example_text.replace('<', "&lt;").replace('>', "&gt;");

            // Remove backticks if they're surrounding the entire content (from
            // literalExpression)
            let trimmed_example = if safe_example.starts_with('`')
                && safe_example.ends_with('`')
                && safe_example.len() > 2
            {
                &safe_example[1..safe_example.len() - 1]
            } else {
                &safe_example
            };

            writeln!(
                html,
                "  <div class=\"option-example\">Example: <pre><code>{trimmed_example}</code></pre></div>"
            )
            .unwrap();
        } else {
            // Check if this is already a code block (surrounded by backticks)
            if example_text.starts_with('`')
                && example_text.ends_with('`')
                && example_text.len() > 2
            {
                // This is inline code - extract the content and properly escape it
                let code_content = &example_text[1..example_text.len() - 1];
                let safe_content = code_content.replace('<', "&lt;").replace('>', "&gt;");
                writeln!(
                    html,
                    "  <div class=\"option-example\">Example: <code>{safe_content}</code></div>"
                )
                .unwrap();
            } else {
                // Regular inline example - still needs escaping
                let safe_example = example_text.replace('<', "&lt;").replace('>', "&gt;");
                writeln!(
                    html,
                    "  <div class=\"option-example\">Example: <code>{safe_example}</code></div>"
                )
                .unwrap();
            }
        }
    } else if let Some(example_val) = &option.example {
        let example_str = example_val.to_string();
        let safe_example = example_str.replace('<', "&lt;").replace('>', "&gt;");
        if example_str.contains('\n') {
            // Multi-line JSON examples need special handling
            writeln!(
                html,
                "  <div class=\"option-example\">Example: <pre><code>{safe_example}</code></pre></div>"
            )
            .unwrap();
        } else {
            // Single-line JSON examples
            writeln!(
                html,
                "  <div class=\"option-example\">Example: <code>{safe_example}</code></div>"
            )
            .unwrap();
        }
    }
}
