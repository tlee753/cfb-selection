//! Builds `data/teams.csv` and `data/games.csv` for the app to embed.
//!
//!   cargo run --features fetch --bin fetch_data                    # fetch from the web
//!   cargo run --features fetch --bin fetch_data -- --html-dir pages # parse pages you saved by hand
//!
//! Sports Reference rate-limits bots (20 requests/min, "jail" if exceeded) and may
//! show a human check. This tool paces itself well under the limit, and if the site
//! pushes back it stops instead of retrying; use `--html-dir` in that case.

use scraper::{ElementRef, Html, Selector};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use std::{env, fs, thread};

const SITE: &str = "https://www.sports-reference.com";
const DEFAULT_YEAR: u32 = 2026;
const TOP_N: usize = 25;
/// 5s between requests = 12/min, comfortably under the 20/min limit.
const MIN_GAP: Duration = Duration::from_secs(5);
const USER_AGENT: &str = "cfb-selection/0.1 (personal hobby project; low volume)";

// ───────────────────────── Page sources ─────────────────────────

enum Source {
    Web {
        client: reqwest::blocking::Client,
        last: Option<Instant>,
    },
    Dir(PathBuf),
}

impl Source {
    fn is_web(&self) -> bool {
        matches!(self, Source::Web { .. })
    }

    /// `name` is the file stem used in `--html-dir` mode (e.g. "ratings", "ohio-state").
    fn page(&mut self, name: &str, url: &str) -> Result<String, String> {
        match self {
            Source::Web { client, last } => {
                if let Some(t) = *last {
                    let elapsed = t.elapsed();
                    if elapsed < MIN_GAP {
                        thread::sleep(MIN_GAP - elapsed);
                    }
                }
                let result = client.get(url).send();
                *last = Some(Instant::now());

                let resp = result.map_err(|e| format!("{url}: {e}"))?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(format!("{url}: HTTP {status}"));
                }
                let body = resp.text().map_err(|e| format!("{url}: {e}"))?;
                if !body.contains("<table") {
                    return Err(format!(
                        "{url}: page has no tables (probably a human-check page)"
                    ));
                }
                Ok(body)
            }
            Source::Dir(dir) => {
                let path = dir.join(format!("{name}.html"));
                fs::read_to_string(&path).map_err(|_| format!("missing {}", path.display()))
            }
        }
    }
}

// ───────────────────────── HTML table parsing ─────────────────────────

struct Cell {
    text: String,
    /// Sports Reference school slug from a /cfb/schools/<slug>/ link in the cell.
    slug: Option<String>,
}

struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<Cell>>,
}

fn clean_text(el: ElementRef) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn school_slug(href: &str) -> Option<String> {
    let rest = href.split("/cfb/schools/").nth(1)?;
    let slug = rest.split('/').next()?;
    (!slug.is_empty()).then(|| slug.to_string())
}

fn slugify(name: &str) -> String {
    name.replace('&', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

/// "(5) Michigan" -> (Some(5), "Michigan")
fn split_rank(raw: &str) -> (Option<u8>, String) {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('(')
        && let Some((num, name)) = rest.split_once(')')
        && let Ok(rank) = num.trim().parse::<u8>()
    {
        return (Some(rank), name.trim().to_string());
    }
    (None, raw.to_string())
}

fn parse_table(html: &str, id: &str) -> Result<Table, String> {
    // Sports Reference tucks many tables inside HTML comments; un-comment everything.
    let cleaned = html.replace("<!--", "").replace("-->", "");
    let doc = Html::parse_document(&cleaned);

    let table_sel = Selector::parse("table").unwrap();
    let table = doc
        .select(&table_sel)
        .find(|t| t.value().id() == Some(id))
        .ok_or_else(|| {
            let ids: Vec<_> = doc.select(&table_sel).filter_map(|t| t.value().id()).collect();
            format!("no <table id=\"{id}\"> on the page; tables found: {ids:?}")
        })?;

    let cell_sel = Selector::parse("th, td").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let head_sel = Selector::parse("thead tr").unwrap();
    let body_sel = Selector::parse("tbody tr").unwrap();

    let headers = table
        .select(&head_sel)
        .last()
        .map(|r| r.select(&cell_sel).map(clean_text).collect())
        .unwrap_or_default();

    let rows = table
        .select(&body_sel)
        .filter(|r| !r.value().classes().any(|c| c == "thead"))
        .map(|r| {
            r.select(&cell_sel)
                .map(|c| Cell {
                    text: clean_text(c),
                    slug: c
                        .select(&a_sel)
                        .filter_map(|a| a.value().attr("href"))
                        .find_map(school_slug),
                })
                .collect()
        })
        .collect();

    Ok(Table { headers, rows })
}

// ───────────────────────── Extraction ─────────────────────────

struct RatedTeam {
    rank: u8,
    slug: String,
    name: String,
    record: String,
}

/// Top 25 from the year's ratings page (the one with an "AP Rank" column).
fn parse_top25(html: &str) -> Result<Vec<RatedTeam>, String> {
    let t = parse_table(html, "ratings")?;
    let col = |names: &[&str]| {
        t.headers
            .iter()
            .position(|h| names.iter().any(|n| h.eq_ignore_ascii_case(n)))
    };
    let missing = |what: &str| format!("ratings table has no {what} column; headers: {:?}", t.headers);

    let school = col(&["School"]).ok_or_else(|| missing("School"))?;
    let ap = col(&["AP Rank", "AP"]).ok_or_else(|| missing("AP Rank"))?;
    let wins = col(&["W"]);
    let losses = col(&["L"]);

    let mut teams: Vec<RatedTeam> = t
        .rows
        .iter()
        .filter_map(|row| {
            let rank = row.get(ap)?.text.parse::<u8>().ok()?;
            let cell = row.get(school)?;
            let record = match (wins.and_then(|i| row.get(i)), losses.and_then(|i| row.get(i))) {
                (Some(w), Some(l)) => format!("({}-{})", w.text, l.text),
                _ => String::new(),
            };
            Some(RatedTeam {
                rank,
                slug: cell.slug.clone().unwrap_or_else(|| slugify(&cell.text)),
                name: cell.text.clone(),
                record,
            })
        })
        .collect();

    teams.sort_by_key(|t| t.rank);
    teams.truncate(TOP_N);
    if teams.is_empty() {
        return Err("found the ratings table but no teams with an AP rank".into());
    }
    Ok(teams)
}

struct GameRow {
    date: String,
    site: String,
    opp_slug: String,
    opp_name: String,
    opp_rank: Option<u8>,
    result: String,
    score: String,
    opp_score: String,
}

/// Same column positions as the site's own CSV:
/// 1 date, 5 site ("@" / "N" / ""), 6 opponent, 8 W/L, 9 pts, 10 opp pts.
fn parse_schedule(html: &str) -> Result<Vec<GameRow>, String> {
    let t = parse_table(html, "schedule")?;
    Ok(t.rows
        .iter()
        .filter(|r| r.len() > 10 && r[0].text != "Rk")
        .map(|r| {
            let (opp_rank, opp_name) = split_rank(&r[6].text);
            GameRow {
                date: r[1].text.clone(),
                site: r[5].text.clone(),
                opp_slug: r[6].slug.clone().unwrap_or_else(|| slugify(&opp_name)),
                opp_name,
                opp_rank,
                result: r[8].text.clone(),
                score: r[9].text.clone(),
                opp_score: r[10].text.clone(),
            }
        })
        .collect())
}

// ───────────────────────── Main ─────────────────────────

struct Args {
    year: u32,
    out: PathBuf,
    html_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        year: DEFAULT_YEAR,
        out: PathBuf::from("data"),
        html_dir: None,
    };
    let mut it = env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--year" => args.year = value()?.parse().map_err(|_| "bad --year".to_string())?,
            "--out" => args.out = value()?.into(),
            "--html-dir" => args.html_dir = Some(value()?.into()),
            "-h" | "--help" => {
                return Err("usage: fetch_data [--year 2026] [--out data] [--html-dir DIR]".into());
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let year = args.year;

    let mut source = match &args.html_dir {
        Some(dir) => Source::Dir(dir.clone()),
        None => Source::Web {
            client: reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .map_err(|e| e.to_string())?,
            last: None,
        },
    };

    let blocked_help = |detail: &str| {
        format!(
            "{detail}\n\nStopping so we don't hammer the site. If Sports Reference is showing a \
             human check, open the pages in your browser, save each one (Ctrl+S, \"Webpage, HTML \
             only\") into a folder named like the list below, and re-run with --html-dir <folder>."
        )
    };

    // 1. Top 25
    let ratings_url = format!("{SITE}/cfb/years/{year}-ratings.html");
    let html = match source.page("ratings", &ratings_url) {
        Ok(h) => h,
        Err(e) if source.is_web() => {
            return Err(blocked_help(&format!(
                "{e}\n\nSave this as ratings.html:\n  {ratings_url}"
            )));
        }
        Err(e) => {
            return Err(format!(
                "{e}\nSave {ratings_url} as ratings.html in that folder, then re-run; I'll list \
                 the team pages to save next."
            ));
        }
    };
    let teams = parse_top25(&html)?;
    println!("Top {}: {}", teams.len(), teams.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "));

    // 2. Each team's schedule (kept in memory; nothing is written unless all succeed)
    let mut all_games: Vec<(String, Vec<GameRow>)> = Vec::new();
    let mut missing: Vec<(String, String)> = Vec::new();

    for team in &teams {
        let url = format!("{SITE}/cfb/schools/{}/{year}-schedule.html", team.slug);
        match source.page(&team.slug, &url) {
            Ok(html) => {
                let games = parse_schedule(&html).map_err(|e| format!("{}: {e}", team.slug))?;
                println!("  {:<2} {:<24} {} games", team.rank, team.name, games.len());
                all_games.push((team.slug.clone(), games));
            }
            Err(e) if source.is_web() => {
                return Err(blocked_help(&format!("{e} (while fetching {})", team.name)));
            }
            Err(_) => missing.push((format!("{}.html", team.slug), url)),
        }
    }

    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|(f, u)| format!("  {f:<32} <- {u}")).collect();
        return Err(format!(
            "{} page(s) still need saving into the folder:\n{}",
            missing.len(),
            list.join("\n")
        ));
    }

    // 3. Write the CSVs the app embeds
    fs::create_dir_all(&args.out).map_err(|e| e.to_string())?;

    let mut w = csv::Writer::from_path(args.out.join("teams.csv")).map_err(|e| e.to_string())?;
    w.write_record(["rank", "slug", "name", "record"]).map_err(|e| e.to_string())?;
    for t in &teams {
        w.write_record([&t.rank.to_string(), &t.slug, &t.name, &t.record])
            .map_err(|e| e.to_string())?;
    }
    w.flush().map_err(|e| e.to_string())?;

    let mut w = csv::Writer::from_path(args.out.join("games.csv")).map_err(|e| e.to_string())?;
    w.write_record(["team", "date", "site", "opp_slug", "opp_name", "opp_rank", "result", "score", "opp_score"])
        .map_err(|e| e.to_string())?;
    for (team, games) in &all_games {
        for g in games {
            let opp_rank = g.opp_rank.map(|r| r.to_string()).unwrap_or_default();
            w.write_record([
                team, &g.date, &g.site, &g.opp_slug, &g.opp_name, &opp_rank, &g.result, &g.score,
                &g.opp_score,
            ])
            .map_err(|e| e.to_string())?;
        }
    }
    w.flush().map_err(|e| e.to_string())?;

    println!("\nWrote {}/teams.csv and {}/games.csv. Rebuild the app to embed them.", args.out.display(), args.out.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}