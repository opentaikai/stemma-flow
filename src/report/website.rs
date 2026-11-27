//! Static family website export.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n::{tr, tr_args};
use crate::report::html::escape_html;

/// One linked person in a website page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteLink {
    pub id: String,
    pub name: String,
}

/// One fact on a person page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFact {
    pub label: String,
    pub date: String,
    pub place: String,
    pub value: String,
}

/// One person page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePerson {
    pub id: String,
    pub name: String,
    pub surname: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub living: bool,
    pub facts: Vec<SiteFact>,
    pub parents: Vec<SiteLink>,
    pub spouses: Vec<SiteLink>,
    pub children: Vec<SiteLink>,
    pub notes: Option<String>,
    pub families: Vec<SiteLink>,
}

/// One family page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFamily {
    pub id: String,
    pub label: String,
    pub husband: Option<SiteLink>,
    pub wife: Option<SiteLink>,
    pub marriage_date: String,
    pub marriage_place: String,
    pub children: Vec<SiteLink>,
}

/// One event using a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteUsage {
    pub label: String,
    pub date: String,
    pub person: Option<SiteLink>,
    pub family: Option<SiteLink>,
}

/// One place page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePlace {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    pub coordinates: String,
    pub usages: Vec<SiteUsage>,
}

/// One source page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteSource {
    pub id: String,
    pub title: String,
    pub author: String,
    pub publication: String,
    pub citations: i64,
    pub repositories: String,
}

/// One repository page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteRepository {
    pub id: String,
    pub name: String,
    pub address: String,
    pub source_titles: Vec<String>,
}

/// One copied media file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteMedia {
    pub id: String,
    pub source: PathBuf,
    pub caption: String,
    pub format: String,
    pub image: bool,
}

/// Everything needed to write a static family website.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsiteReport {
    pub title: String,
    pub tree_name: String,
    pub generated_at: String,
    pub people: Vec<SitePerson>,
    pub families: Vec<SiteFamily>,
    pub places: Vec<SitePlace>,
    pub sources: Vec<SiteSource>,
    pub repositories: Vec<SiteRepository>,
    pub media: Vec<SiteMedia>,
    pub chart_svg: Option<String>,
}

/// Website write statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebsiteStats {
    pub pages: usize,
    pub media: usize,
    pub bytes: usize,
}

/// Writes the complete static site into `directory`.
pub fn write_site(directory: &Path, report: &WebsiteReport) -> Result<WebsiteStats, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for folder in [
        "assets",
        "people",
        "families",
        "places",
        "sources",
        "repositories",
        "surnames",
        "media",
    ] {
        fs::create_dir_all(directory.join(folder)).map_err(|error| error.to_string())?;
    }
    if report.chart_svg.is_some() {
        fs::create_dir_all(directory.join("charts")).map_err(|error| error.to_string())?;
    }
    let mut stats = WebsiteStats::default();

    let person_pages: HashMap<&str, String> = report
        .people
        .iter()
        .enumerate()
        .map(|(index, person)| (person.id.as_str(), format!("p{:04}.html", index + 1)))
        .collect();
    let family_pages: HashMap<&str, String> = report
        .families
        .iter()
        .enumerate()
        .map(|(index, family)| (family.id.as_str(), format!("f{:04}.html", index + 1)))
        .collect();
    let place_pages: HashMap<&str, String> = report
        .places
        .iter()
        .enumerate()
        .map(|(index, place)| (place.id.as_str(), format!("pl{:04}.html", index + 1)))
        .collect();
    let source_pages: HashMap<&str, String> = report
        .sources
        .iter()
        .enumerate()
        .map(|(index, source)| (source.id.as_str(), format!("s{:04}.html", index + 1)))
        .collect();
    let repository_pages: HashMap<&str, String> = report
        .repositories
        .iter()
        .enumerate()
        .map(|(index, repository)| (repository.id.as_str(), format!("r{:04}.html", index + 1)))
        .collect();

    // Surname index pages.
    let mut surnames: BTreeMap<String, Vec<&SitePerson>> = BTreeMap::new();
    for person in &report.people {
        let surname = person.surname.trim();
        let key = if surname.is_empty() {
            "(unknown)".to_string()
        } else {
            surname.to_uppercase()
        };
        surnames.entry(key).or_default().push(person);
    }
    let surname_pages: HashMap<String, String> = surnames
        .keys()
        .enumerate()
        .map(|(index, surname)| (surname.clone(), format!("s{:04}.html", index + 1)))
        .collect();

    // Copy media.
    let mut media_paths: Vec<Option<String>> = Vec::with_capacity(report.media.len());
    for (index, media) in report.media.iter().enumerate() {
        if !media.source.is_file() {
            media_paths.push(None);
            continue;
        }
        let file_name = media
            .source
            .file_name()
            .and_then(|name| name.to_str())
            .map(sanitize_file_stem)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "media".to_string());
        let extension = media
            .source
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!(".{extension}"))
            .unwrap_or_default();
        let relative = format!("m{:04}-{file_name}{extension}", index + 1);
        if fs::copy(&media.source, directory.join("media").join(&relative)).is_ok() {
            media_paths.push(Some(relative));
            stats.media += 1;
        } else {
            media_paths.push(None);
        }
    }

    let page = |title: &str, prefix: &str, body: String| site_page(title, prefix, report, body);

    // Index.
    let mut index = String::new();
    index.push_str("<section class=\"hero\"><h1>");
    index.push_str(&escape_html(&report.title));
    index.push_str("</h1><p class=\"muted\">");
    index.push_str(&escape_html(&report.tree_name));
    index.push_str(" \u{00b7} ");
    index.push_str(&escape_html(&report.generated_at));
    index.push_str("</p><p>");
    index.push_str(&tr_args(
        "report.website.index_counts",
        &[
            ("people", report.people.len().to_string()),
            ("families", report.families.len().to_string()),
            ("places", report.places.len().to_string()),
            ("sources", report.sources.len().to_string()),
            ("media", report.media.len().to_string()),
        ],
    ));
    index.push_str("</p></section>");
    index.push_str(&format!(
        "<section class=\"search\"><h2>{}</h2>",
        tr("report.website.search")
    ));
    index.push_str(&format!(
        "<input id=\"site-search\" type=\"search\" placeholder=\"{}\">",
        tr("report.website.search_people")
    ));
    index.push_str("<ul id=\"search-results\"></ul></section>");
    index.push_str(&format!(
        "<section><h2>{}</h2><ul class=\"columns\">",
        tr("report.website.surnames")
    ));
    for (surname, url) in &surname_pages {
        index.push_str("<li><a href=\"surnames/");
        index.push_str(url);
        index.push_str("\">");
        index.push_str(&escape_html(surname));
        index.push_str("</a> <span class=\"muted\">(");
        index.push_str(&surnames[surname].len().to_string());
        index.push_str(")</span></li>");
    }
    index.push_str("</ul></section>");
    index.push_str(
        "<script src=\"search-data.js\"></script><script src=\"assets/search.js\"></script>",
    );
    write_page(
        &directory.join("index.html"),
        &page(&report.title, "", index),
        &mut stats,
    )?;

    // Surname pages.
    for (surname, url) in &surname_pages {
        let mut body = String::new();
        body.push_str("<h1>");
        body.push_str(&escape_html(surname));
        body.push_str("</h1><ul>");
        for person in &surnames[surname] {
            body.push_str("<li>");
            person_link(
                &mut body,
                &person.id,
                &person.name,
                &person_pages,
                "people/",
            );
            body.push_str("</li>");
        }
        body.push_str("</ul>");
        write_page(
            &directory.join("surnames").join(url),
            &page(surname, "../", body),
            &mut stats,
        )?;
    }

    // Person pages.
    for person in &report.people {
        let Some(file) = person_pages.get(person.id.as_str()) else {
            continue;
        };
        let mut body = String::new();
        body.push_str("<article class=\"person\"><h1>");
        body.push_str(&escape_html(&person.name));
        body.push_str("</h1>");
        if person.living {
            body.push_str(&format!(
                "<p class=\"living\">{}</p>",
                tr("report.common.living")
            ));
        } else {
            body.push_str("<dl class=\"vitals\">");
            push_vital(
                &mut body,
                tr("report.common.birth"),
                &person.birth,
                &person.birth_place,
            );
            push_vital(
                &mut body,
                tr("report.common.death"),
                &person.death,
                &person.death_place,
            );
            body.push_str("</dl>");
            if !person.facts.is_empty() {
                body.push_str(&format!("<section><h2>{}</h2><table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>", tr("report.common.facts"), tr("report.common.fact"), tr("report.common.date"), tr("report.common.place"), tr("report.common.details")));
                for fact in &person.facts {
                    body.push_str("<tr><td>");
                    body.push_str(&escape_html(&fact.label));
                    body.push_str("</td><td>");
                    body.push_str(&escape_html(&fact.date));
                    body.push_str("</td><td>");
                    body.push_str(&escape_html(&fact.place));
                    body.push_str("</td><td>");
                    body.push_str(&escape_html(&fact.value));
                    body.push_str("</td></tr>");
                }
                body.push_str("</tbody></table></section>");
            }
        }
        relative_section(
            &mut body,
            tr("report.common.parents"),
            &person.parents,
            &person_pages,
            "people/",
        );
        relative_section(
            &mut body,
            tr("report.common.spouses"),
            &person.spouses,
            &person_pages,
            "people/",
        );
        relative_section(
            &mut body,
            tr("report.common.children"),
            &person.children,
            &person_pages,
            "people/",
        );
        if !person.families.is_empty() {
            body.push_str(&format!(
                "<section><h2>{}</h2><ul>",
                tr("report.common.families")
            ));
            for family in &person.families {
                body.push_str("<li>");
                if let Some(file) = family_pages.get(family.id.as_str()) {
                    body.push_str("<a href=\"../families/");
                    body.push_str(file);
                    body.push_str("\">");
                    body.push_str(&escape_html(&family.name));
                    body.push_str("</a>");
                } else {
                    body.push_str(&escape_html(&family.name));
                }
                body.push_str("</li>");
            }
            body.push_str("</ul></section>");
        }
        if !person.living
            && let Some(notes) = &person.notes
            && !notes.trim().is_empty()
        {
            body.push_str(&format!(
                "<section class=\"notes\"><h2>{}</h2>",
                tr("report.common.notes")
            ));
            for paragraph in notes.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
                body.push_str("<p>");
                body.push_str(&escape_html(paragraph).replace('\n', "<br>"));
                body.push_str("</p>");
            }
            body.push_str("</section>");
        }
        body.push_str("</article>");
        write_page(
            &directory.join("people").join(file),
            &page(&person.name, "../", body),
            &mut stats,
        )?;
    }

    // Family pages.
    for family in &report.families {
        let Some(file) = family_pages.get(family.id.as_str()) else {
            continue;
        };
        let mut body = String::new();
        body.push_str("<h1>");
        body.push_str(&escape_html(&family.label));
        body.push_str("</h1><dl class=\"vitals\">");
        if let Some(husband) = &family.husband {
            body.push_str(&format!(
                "<div><dt>{}</dt><dd>",
                tr("report.common.husband")
            ));
            person_link(
                &mut body,
                &husband.id,
                &husband.name,
                &person_pages,
                "../people/",
            );
            body.push_str("</dd></div>");
        }
        if let Some(wife) = &family.wife {
            body.push_str(&format!("<div><dt>{}</dt><dd>", tr("report.common.wife")));
            person_link(&mut body, &wife.id, &wife.name, &person_pages, "../people/");
            body.push_str("</dd></div>");
        }
        let marriage = event_cell(&family.marriage_date, &family.marriage_place);
        if !marriage.is_empty() {
            body.push_str(&format!(
                "<div><dt>{}</dt><dd>",
                tr("report.common.marriage")
            ));
            body.push_str(&escape_html(&marriage));
            body.push_str("</dd></div>");
        }
        body.push_str("</dl>");
        if !family.children.is_empty() {
            body.push_str(&format!(
                "<section><h2>{}</h2><ul>",
                tr("report.common.children")
            ));
            for child in &family.children {
                body.push_str("<li>");
                person_link(
                    &mut body,
                    &child.id,
                    &child.name,
                    &person_pages,
                    "../people/",
                );
                body.push_str("</li>");
            }
            body.push_str("</ul></section>");
        }
        write_page(
            &directory.join("families").join(file),
            &page(&family.label, "../", body),
            &mut stats,
        )?;
    }

    // Places.
    let mut place_index = String::new();
    place_index.push_str(&format!("<h1>{}</h1><ul>", tr("report.website.places")));
    for place in &report.places {
        let Some(file) = place_pages.get(place.id.as_str()) else {
            continue;
        };
        place_index.push_str("<li><a href=\"");
        place_index.push_str(file);
        place_index.push_str("\">");
        place_index.push_str(&escape_html(&place.name));
        place_index.push_str("</a> <span class=\"muted\">(");
        place_index.push_str(&place.usages.len().to_string());
        place_index.push_str(" events)</span></li>");

        let mut body = String::new();
        body.push_str("<h1>");
        body.push_str(&escape_html(&place.name));
        body.push_str("</h1>");
        if !place.coordinates.is_empty() {
            body.push_str("<p class=\"muted\">");
            body.push_str(&escape_html(&place.coordinates));
            body.push_str("</p>");
        }
        if let Some(notes) = &place.notes
            && !notes.trim().is_empty()
        {
            body.push_str("<p>");
            body.push_str(&escape_html(notes));
            body.push_str("</p>");
        }
        if place.usages.is_empty() {
            body.push_str(&format!(
                "<p class=\"empty\">{}</p>",
                tr("report.website.no_events")
            ));
        } else {
            body.push_str(
                "&format!(\"<table><thead><tr><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>\", tr(\"report.common.event\"), tr(\"report.common.date\"), tr(\"report.common.person\"))",
            );
            for usage in &place.usages {
                body.push_str("<tr><td>");
                body.push_str(&escape_html(&usage.label));
                body.push_str("</td><td>");
                body.push_str(&escape_html(&usage.date));
                body.push_str("</td><td>");
                if let Some(person) = &usage.person {
                    person_link(
                        &mut body,
                        &person.id,
                        &person.name,
                        &person_pages,
                        "../people/",
                    );
                } else if let Some(family) = &usage.family {
                    if let Some(file) = family_pages.get(family.id.as_str()) {
                        body.push_str("<a href=\"../families/");
                        body.push_str(file);
                        body.push_str("\">");
                        body.push_str(&escape_html(&family.name));
                        body.push_str("</a>");
                    } else {
                        body.push_str(&escape_html(&family.name));
                    }
                }
                body.push_str("</td></tr>");
            }
            body.push_str("</tbody></table>");
        }
        write_page(
            &directory.join("places").join(file),
            &page(&place.name, "../", body),
            &mut stats,
        )?;
    }
    place_index.push_str("</ul>");
    write_page(
        &directory.join("places").join("index.html"),
        &page(tr("report.website.places"), "../", place_index),
        &mut stats,
    )?;

    // Sources.
    let mut source_index = String::new();
    source_index.push_str(&format!("<h1>{}</h1><ul>", tr("report.website.sources")));
    for source in &report.sources {
        let Some(file) = source_pages.get(source.id.as_str()) else {
            continue;
        };
        source_index.push_str("<li><a href=\"");
        source_index.push_str(file);
        source_index.push_str("\">");
        source_index.push_str(&escape_html(&source.title));
        source_index.push_str("</a> <span class=\"muted\">(");
        source_index.push_str(&source.citations.to_string());
        source_index.push_str(" citations)</span></li>");

        let mut body = String::new();
        body.push_str("<h1>");
        body.push_str(&escape_html(&source.title));
        body.push_str("</h1><dl class=\"vitals\">");
        if !source.author.is_empty() {
            body.push_str(&format!("<div><dt>{}</dt><dd>", tr("report.common.author")));
            body.push_str(&escape_html(&source.author));
            body.push_str("</dd></div>");
        }
        if !source.publication.is_empty() {
            body.push_str(&format!(
                "<div><dt>{}</dt><dd>",
                tr("report.common.publication")
            ));
            body.push_str(&escape_html(&source.publication));
            body.push_str("</dd></div>");
        }
        if !source.repositories.is_empty() {
            body.push_str(&format!(
                "<div><dt>{}</dt><dd>",
                tr("report.common.repositories")
            ));
            body.push_str(&escape_html(&source.repositories));
            body.push_str("</dd></div>");
        }
        body.push_str("</dl>");
        write_page(
            &directory.join("sources").join(file),
            &page(&source.title, "../", body),
            &mut stats,
        )?;
    }
    source_index.push_str("</ul>");
    write_page(
        &directory.join("sources").join("index.html"),
        &page(tr("report.website.sources"), "../", source_index),
        &mut stats,
    )?;

    // Repositories.
    let mut repository_index = String::new();
    repository_index.push_str(&format!(
        "<h1>{}</h1><ul>",
        tr("report.website.repositories")
    ));
    for repository in &report.repositories {
        let Some(file) = repository_pages.get(repository.id.as_str()) else {
            continue;
        };
        repository_index.push_str("<li><a href=\"");
        repository_index.push_str(file);
        repository_index.push_str("\">");
        repository_index.push_str(&escape_html(&repository.name));
        repository_index.push_str("</a></li>");

        let mut body = String::new();
        body.push_str("<h1>");
        body.push_str(&escape_html(&repository.name));
        body.push_str("</h1>");
        if !repository.address.is_empty() {
            body.push_str("<p>");
            body.push_str(&escape_html(&repository.address));
            body.push_str("</p>");
        }
        if repository.source_titles.is_empty() {
            body.push_str(&format!(
                "<p class=\"empty\">{}</p>",
                tr("report.website.no_linked_sources")
            ));
        } else {
            body.push_str(&format!(
                "<section><h2>{}</h2><ul>",
                tr("report.common.sources")
            ));
            for title in &repository.source_titles {
                body.push_str("<li>");
                body.push_str(&escape_html(title));
                body.push_str("</li>");
            }
            body.push_str("</ul></section>");
        }
        write_page(
            &directory.join("repositories").join(file),
            &page(&repository.name, "../", body),
            &mut stats,
        )?;
    }
    repository_index.push_str("</ul>");
    write_page(
        &directory.join("repositories").join("index.html"),
        &page(tr("report.website.repositories"), "../", repository_index),
        &mut stats,
    )?;

    // Media gallery.
    let mut gallery = String::new();
    gallery.push_str(&format!("<h1>{}</h1>", tr("report.website.media")));
    let copied: Vec<(&SiteMedia, &String)> = report
        .media
        .iter()
        .zip(media_paths.iter())
        .filter_map(|(media, path)| path.as_ref().map(|path| (media, path)))
        .collect();
    if copied.is_empty() {
        gallery.push_str(&format!(
            "<p class=\"empty\">{}</p>",
            tr("report.common.no_media")
        ));
    } else {
        gallery.push_str("<div class=\"gallery\">");
        for (media, path) in &copied {
            gallery.push_str("<figure>");
            if media.image {
                gallery.push_str("<img src=\"");
                gallery.push_str(&escape_html(path));
                gallery.push_str("\" alt=\"");
                gallery.push_str(&escape_html(&media.caption));
                gallery.push_str("\" loading=\"lazy\">");
            } else {
                gallery.push_str("<a class=\"file\" href=\"");
                gallery.push_str(&escape_html(path));
                gallery.push_str("\">");
                gallery.push_str(&escape_html(&media.format));
                gallery.push_str("</a>");
            }
            gallery.push_str("<figcaption>");
            gallery.push_str(&escape_html(if media.caption.is_empty() {
                path
            } else {
                &media.caption
            }));
            gallery.push_str("</figcaption></figure>");
        }
        gallery.push_str("</div>");
    }
    write_page(
        &directory.join("media").join("index.html"),
        &page(tr("report.website.media"), "../", gallery),
        &mut stats,
    )?;

    // Optional chart.
    if let Some(chart) = &report.chart_svg {
        let chart_page = "&format!(\"<h1>{}</h1><p><img class=\\\"chart\\\" src=\\\"pedigree.svg\\\" alt=\\\"{}\\\"></p>\", tr(\"report.website.pedigree_chart\"), tr(\"report.website.pedigree_chart\"))";
        write_page(
            &directory.join("charts").join("index.html"),
            &page(
                tr("report.website.pedigree_chart"),
                "../",
                chart_page.to_string(),
            ),
            &mut stats,
        )?;
        fs::write(
            directory.join("charts").join("pedigree.svg"),
            chart.as_bytes(),
        )
        .map_err(|error| error.to_string())?;
        stats.bytes += chart.len();
    }

    // Assets and search.
    fs::write(directory.join("assets").join("site.css"), CSS.as_bytes())
        .map_err(|error| error.to_string())?;
    stats.bytes += CSS.len();
    fs::write(
        directory.join("assets").join("search.js"),
        SEARCH_JS.as_bytes(),
    )
    .map_err(|error| error.to_string())?;
    stats.bytes += SEARCH_JS.len();
    let entries: Vec<serde_json::Value> = report
        .people
        .iter()
        .filter_map(|person| {
            let url = person_pages.get(person.id.as_str())?;
            let dates = if person.living {
                "Living".to_string()
            } else {
                event_cell(&person.birth, &person.death)
            };
            Some(serde_json::json!({
                "name": person.name,
                "dates": dates,
                "url": format!("people/{url}"),
            }))
        })
        .collect();
    let data = format!(
        "window.STEMMA_SEARCH = {};",
        serde_json::to_string(&entries).map_err(|error| error.to_string())?
    );
    fs::write(directory.join("search-data.js"), data.as_bytes())
        .map_err(|error| error.to_string())?;
    stats.bytes += data.len();

    Ok(stats)
}

fn write_page(path: &Path, html: &str, stats: &mut WebsiteStats) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    stats.pages += 1;
    stats.bytes += html.len();
    Ok(())
}

fn site_page(title: &str, prefix: &str, report: &WebsiteReport, body: String) -> String {
    let mut html = String::with_capacity(body.len() + 2048);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(title));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.title));
    html.push_str("</title><link rel=\"stylesheet\" href=\"");
    html.push_str(prefix);
    html.push_str(
        "assets/site.css\"></head><body><header class=\"site-header\"><a class=\"brand\" href=\"",
    );
    html.push_str(prefix);
    html.push_str("index.html\">");
    html.push_str(&escape_html(&report.title));
    html.push_str("</a><nav><a href=\"");
    html.push_str(prefix);
    html.push_str("index.html\">");
    html.push_str(&escape_html(tr("report.website.home")));
    html.push_str("</a><a href=\"");
    html.push_str(prefix);
    html.push_str("surnames/\">");
    html.push_str(&escape_html(tr("report.website.surnames")));
    html.push_str("</a><a href=\"");
    html.push_str(prefix);
    html.push_str("places/\">");
    html.push_str(&escape_html(tr("report.website.places")));
    html.push_str("</a><a href=\"");
    html.push_str(prefix);
    html.push_str("sources/\">");
    html.push_str(&escape_html(tr("report.website.sources")));
    html.push_str("</a><a href=\"");
    html.push_str(prefix);
    html.push_str("repositories/\">");
    html.push_str(&escape_html(tr("report.website.repositories")));
    html.push_str("</a><a href=\"");
    html.push_str(prefix);
    html.push_str("media/\">");
    html.push_str(&escape_html(tr("report.website.media")));
    html.push_str("</a>");
    if report.chart_svg.is_some() {
        html.push_str("<a href=\"");
        html.push_str(prefix);
        html.push_str("charts/\">");
        html.push_str(&escape_html(tr("report.website.chart")));
        html.push_str("</a>");
    }
    html.push_str("</nav></header><main>");
    html.push_str(&body);
    html.push_str("</main><footer>");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str("</footer></body></html>");
    html
}

fn person_link(
    html: &mut String,
    id: &str,
    name: &str,
    pages: &HashMap<&str, String>,
    prefix: &str,
) {
    match pages.get(id) {
        Some(file) => {
            html.push_str("<a href=\"");
            html.push_str(prefix);
            html.push_str(file);
            html.push_str("\">");
            html.push_str(&escape_html(name));
            html.push_str("</a>");
        }
        None => html.push_str(&escape_html(name)),
    }
}

fn relative_section(
    html: &mut String,
    title: &str,
    links: &[SiteLink],
    pages: &HashMap<&str, String>,
    prefix: &str,
) {
    html.push_str("<section><h2>");
    html.push_str(title);
    html.push_str("</h2><ul>");
    for link in links {
        html.push_str("<li>");
        person_link(html, &link.id, &link.name, pages, prefix);
        html.push_str("</li>");
    }
    html.push_str("</ul></section>");
}

fn push_vital(html: &mut String, label: &str, date: &str, place: &str) {
    let value = event_cell(date, place);
    html.push_str("<div><dt>");
    html.push_str(label);
    html.push_str("</dt><dd>");
    if value.is_empty() {
        html.push_str(&format!(
            "<span class=\"unknown\">{}</span>",
            tr("report.common.unknown")
        ));
    } else {
        html.push_str(&escape_html(&value));
    }
    html.push_str("</dd></div>");
}

fn event_cell(date: &str, place: &str) -> String {
    let date = date.trim();
    let place = place.trim();
    match (date.is_empty(), place.is_empty()) {
        (true, true) => String::new(),
        (false, true) => date.to_string(),
        (true, false) => place.to_string(),
        (false, false) => format!("{date} \u{00b7} {place}"),
    }
}

fn sanitize_file_stem(value: &str) -> String {
    let value = value
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(value);
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

const CSS: &str = r#"
:root { color-scheme: light; }
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; color: #1e293b; background: #f8fafc; line-height: 1.5; }
a { color: #1d4ed8; text-decoration: none; }
a:hover { text-decoration: underline; }
.site-header { display: flex; flex-wrap: wrap; gap: 16px; align-items: center; justify-content: space-between; padding: 14px 24px; background: #1e3a8a; color: white; }
.site-header a { color: white; }
.brand { font-size: 18px; font-weight: 700; }
.site-header nav { display: flex; flex-wrap: wrap; gap: 14px; font-size: 14px; }
main { max-width: 1100px; margin: 0 auto; padding: 28px 20px; }
footer { max-width: 1100px; margin: 0 auto; padding: 16px 20px 28px; color: #64748b; font-size: 12px; }
h1 { margin: 0 0 12px; }
h2 { margin: 24px 0 8px; font-size: 1.1rem; }
.hero { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 24px; }
.muted, .unknown { color: #64748b; }
.living { font-weight: 600; color: #475569; }
.search input { width: 100%; max-width: 420px; padding: 9px 12px; border: 1px solid #cbd5e1; border-radius: 8px; font: inherit; }
#search-results { padding-left: 20px; }
#search-results li { margin: 6px 0; }
.columns { columns: 2 240px; padding-left: 20px; }
.vitals { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; }
.vitals div { display: contents; }
.vitals dt { color: #64748b; }
.vitals dd { margin: 0; }
table { width: 100%; border-collapse: collapse; background: white; font-size: 14px; }
th, td { text-align: left; padding: 7px 9px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
.gallery { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
.gallery figure { margin: 0; background: white; border: 1px solid #e2e8f0; border-radius: 10px; overflow: hidden; }
.gallery img { display: block; width: 100%; height: 180px; object-fit: cover; background: #e2e8f0; }
.gallery .file { display: flex; align-items: center; justify-content: center; height: 180px; background: #e2e8f0; font-weight: 600; }
figcaption { padding: 8px 10px; font-size: 13px; }
.chart { max-width: 100%; height: auto; }
.empty { color: #64748b; }
@media print { .site-header, footer { display: none; } body { background: white; } main { max-width: none; padding: 0; } }
"#;

const SEARCH_JS: &str = r#"
(function () {
  var input = document.getElementById('site-search');
  var results = document.getElementById('search-results');
  if (!input || !results || !window.STEMMA_SEARCH) return;
  function render() {
    var needle = input.value.trim().toLowerCase();
    results.innerHTML = '';
    if (!needle) return;
    var shown = 0;
    window.STEMMA_SEARCH.forEach(function (person) {
      if (shown >= 30) return;
      var haystack = (person.name + ' ' + (person.dates || '')).toLowerCase();
      if (haystack.indexOf(needle) === -1) return;
      var item = document.createElement('li');
      var link = document.createElement('a');
      link.href = person.url;
      link.textContent = person.name + (person.dates ? ' — ' + person.dates : '');
      item.appendChild(link);
      results.appendChild(item);
      shown += 1;
    });
  }
  input.addEventListener('input', render);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, name: &str, living: bool) -> SitePerson {
        SitePerson {
            id: id.to_string(),
            name: name.to_string(),
            surname: "King".to_string(),
            birth: if living {
                "2000".to_string()
            } else {
                "1815".to_string()
            },
            birth_place: "London".to_string(),
            death: String::new(),
            death_place: String::new(),
            living,
            facts: vec![SiteFact {
                label: "Occupation".to_string(),
                date: "1840".to_string(),
                place: "London".to_string(),
                value: "Teacher".to_string(),
            }],
            parents: Vec::new(),
            spouses: Vec::new(),
            children: Vec::new(),
            notes: Some("A note".to_string()),
            families: Vec::new(),
        }
    }

    fn report() -> WebsiteReport {
        WebsiteReport {
            title: "King family".to_string(),
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            people: vec![
                person("p1", "Ada King", false),
                person("p2", "Living King", true),
            ],
            families: vec![SiteFamily {
                id: "f1".to_string(),
                label: "Ada & Living".to_string(),
                husband: Some(SiteLink {
                    id: "p1".to_string(),
                    name: "Ada King".to_string(),
                }),
                wife: Some(SiteLink {
                    id: "p2".to_string(),
                    name: "Living King".to_string(),
                }),
                marriage_date: "1838".to_string(),
                marriage_place: "London".to_string(),
                children: Vec::new(),
            }],
            places: vec![SitePlace {
                id: "pl1".to_string(),
                name: "London".to_string(),
                notes: None,
                coordinates: String::new(),
                usages: vec![SiteUsage {
                    label: "Birth".to_string(),
                    date: "1815".to_string(),
                    person: Some(SiteLink {
                        id: "p1".to_string(),
                        name: "Ada King".to_string(),
                    }),
                    family: None,
                }],
            }],
            sources: vec![SiteSource {
                id: "s1".to_string(),
                title: "Parish register".to_string(),
                author: "St Mary".to_string(),
                publication: "1900".to_string(),
                citations: 1,
                repositories: "Archive".to_string(),
            }],
            repositories: vec![SiteRepository {
                id: "r1".to_string(),
                name: "Archive".to_string(),
                address: "1 High St".to_string(),
                source_titles: vec!["Parish register".to_string()],
            }],
            media: Vec::new(),
            chart_svg: Some("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_string()),
        }
    }

    #[test]
    fn writes_index_people_families_and_places() {
        let dir = std::env::temp_dir().join(format!("stemma-site-{}", uuid::Uuid::new_v4()));
        let stats = write_site(&dir, &report()).expect("site");
        assert!(stats.pages >= 7);
        let index = fs::read_to_string(dir.join("index.html")).expect("index");
        assert!(index.contains("King family"));
        assert!(index.contains("site-search"));
        assert!(index.contains("search-data.js"));
        assert!(dir.join("people/p0001.html").is_file());
        let person = fs::read_to_string(dir.join("people/p0001.html")).expect("person");
        assert!(person.contains("Ada King"));
        assert!(person.contains("Occupation"));
        assert!(person.contains("A note"));
        let living = fs::read_to_string(dir.join("people/p0002.html")).expect("living");
        assert!(living.contains("Living"));
        assert!(!living.contains("2000"));
        assert!(dir.join("families/f0001.html").is_file());
        assert!(dir.join("places/pl0001.html").is_file());
        assert!(dir.join("sources/s0001.html").is_file());
        assert!(dir.join("repositories/r0001.html").is_file());
        assert!(dir.join("surnames/s0001.html").is_file());
        assert!(dir.join("media/index.html").is_file());
        assert!(dir.join("charts/pedigree.svg").is_file());
        assert!(dir.join("assets/site.css").is_file());
        let search = fs::read_to_string(dir.join("search-data.js")).expect("search");
        assert!(search.contains("Ada King"));
        assert!(search.contains("Living"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn copies_media_and_skips_missing_files() {
        let dir = std::env::temp_dir().join(format!("stemma-site-{}", uuid::Uuid::new_v4()));
        let source = dir.join("photo.png");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(&source, b"not really a png").expect("media");
        let mut report = report();
        report.media = vec![
            SiteMedia {
                id: "m1".to_string(),
                source,
                caption: "Photo".to_string(),
                format: "png".to_string(),
                image: true,
            },
            SiteMedia {
                id: "m2".to_string(),
                source: dir.join("missing.jpg"),
                caption: "Missing".to_string(),
                format: "jpg".to_string(),
                image: true,
            },
        ];
        let stats = write_site(&dir.join("site"), &report).expect("site");
        assert_eq!(stats.media, 1);
        assert!(dir.join("site/media/m0001-photo.png").is_file());
        let gallery = fs::read_to_string(dir.join("site/media/index.html")).expect("gallery");
        assert!(gallery.contains("Photo"));
        assert!(!gallery.contains("Missing"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_site_still_has_index_and_assets() {
        let dir = std::env::temp_dir().join(format!("stemma-site-{}", uuid::Uuid::new_v4()));
        let report = WebsiteReport {
            title: "Empty".to_string(),
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            people: Vec::new(),
            families: Vec::new(),
            places: Vec::new(),
            sources: Vec::new(),
            repositories: Vec::new(),
            media: Vec::new(),
            chart_svg: None,
        };
        write_site(&dir, &report).expect("site");
        assert!(dir.join("index.html").is_file());
        assert!(dir.join("assets/site.css").is_file());
        let index = fs::read_to_string(dir.join("index.html")).expect("index");
        assert!(index.contains("0 people"));
        let _ = fs::remove_dir_all(dir);
    }
}
