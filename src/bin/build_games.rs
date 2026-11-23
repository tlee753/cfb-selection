//! Builds the app's data files from CSVs you download by hand.
//!
//!   cargo run --bin build_games            # reads/writes ./data
//!   cargo run --bin build_games -- some/dir
//!
//! Needs, in the data folder:
//!   cfp.csv, ap.csv         the raw poll tables (Wk,Date,Rk,School,Prev,Chng,1st,Conf,This Week)
//!                           where School looks like "Miami (FL) (10-1)"
//!   <team>.csv              one Sports Reference schedule CSV per team, named after the
//!                           team's slug ("ohio-state.csv") or its name ("usc.csv")
//!
//! Produces, in the same folder:
//!   1-cfp.csv, 2-ap.csv     rank,slug,name,record
//!   3-games.csv             team,date,site,opp_slug,opp_name,opp_rank,result,score,opp_score
//!
//! Slugs are generated from the school name ("Texas A&M" -> "texas-am").
//! Every team in either poll is looked up for a schedule, so a team in both is read once.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Team {
    rank: u8,
    slug: String,
    name: String,
    record: String,
}

/// Sports Reference slug -> the slug we generate from the poll name.
/// Used to find schedule files ("brigham-young.csv" for BYU) and to match opponents
/// listed as "Brigham Young" in schedules back to the poll team "BYU".
/// Add a line here whenever a poll name and Sports Reference disagree.
const ALIASES: &[(&str, &str)] = &[
    ("brigham-young", "byu"),
    ("southern-california", "usc"),
    ("mississippi", "ole-miss"),
    ("louisiana-state", "lsu"),
    ("southern-methodist", "smu"),
    ("texas-christian", "tcu"),
    ("pittsburgh", "pitt"),
];

/// "Texas A&M" -> "texas-am", "Miami (FL)" -> "miami-fl", "Ohio State" -> "ohio-state"
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name
        .chars()
        .filter(|c| !matches!(c, '&' | '\'' | '’' | '.'))
    {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
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

/// "Miami (FL) (10-1)" -> ("Miami (FL)", "10-1"). "Miami (FL)" alone -> ("Miami (FL)", "").
fn split_record(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if raw.ends_with(')')
        && let Some(open) = raw.rfind('(')
    {
        let inner = &raw[open + 1..raw.len() - 1];
        if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return (raw[..open].trim().to_string(), inner.to_string());
        }
    }
    (raw.to_string(), String::new())
}

fn read_records(path: &Path) -> Result<Vec<csv::StringRecord>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(reader.records().filter_map(Result::ok).collect())
}

/// Reads a raw poll table, using the "Rk" and "School" columns.
fn read_poll(path: &Path) -> Result<Vec<Team>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;

    let headers = reader
        .headers()
        .map_err(|e| format!("{}: {e}", path.display()))?
        .clone();
    let col = |name: &str| -> Result<usize, String> {
        headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{}: no \"{name}\" column", path.display()))
    };
    let rk_col = col("Rk")?;
    let school_col = col("School")?;

    let mut teams = Vec::new();
    for rec in reader.records().filter_map(Result::ok) {
        let Some(rank) = rec.get(rk_col).and_then(|r| r.trim().parse::<u8>().ok()) else {
            continue;
        };
        let (name, record) = split_record(rec.get(school_col).unwrap_or(""));
        if name.is_empty() {
            continue;
        }
        if record.is_empty() {
            eprintln!("warning: {}: no record found for {name}", path.display());
        }
        teams.push(Team {
            rank,
            slug: slugify(&name),
            name,
            record,
        });
    }
    teams.sort_by_key(|t| t.rank);
    Ok(teams)
}

fn write_poll(path: &Path, teams: &[Team]) -> Result<(), String> {
    let mut writer =
        csv::Writer::from_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
    writer
        .write_record(["rank", "slug", "name", "record"])
        .map_err(|e| e.to_string())?;
    for t in teams {
        writer
            .write_record([&t.rank.to_string(), &t.slug, &t.name, &t.record])
            .map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    println!("Wrote {} teams to {}", teams.len(), path.display());
    Ok(())
}

fn run() -> Result<(), String> {
    let dir = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "data".into());

    // Raw polls in, clean polls out
    let cfp = read_poll(&dir.join("cfp.csv"))?;
    let ap = read_poll(&dir.join("ap.csv"))?;
    write_poll(&dir.join("1-cfp.csv"), &cfp)?;
    write_poll(&dir.join("2-ap.csv"), &ap)?;

    // Ranked teams from both polls, de-duplicated
    let mut teams: Vec<&Team> = Vec::new();
    for team in cfp.iter().chain(ap.iter()) {
        if !teams.iter().any(|t| t.slug == team.slug) {
            teams.push(team);
        }
    }
    if teams.is_empty() {
        return Err(format!(
            "no teams found in {}/cfp.csv or ap.csv",
            dir.display()
        ));
    }

    // Opponent names in the schedules ("USC", "Ole Miss", "BYU") -> the slugs the app uses.
    let by_name: HashMap<String, String> = teams
        .iter()
        .map(|t| (slugify(&t.name), t.slug.clone()))
        .collect();
    let resolve = |opponent: &str| {
        let key = slugify(opponent);
        if let Some((_, ours)) = ALIASES.iter().find(|(sr, _)| *sr == key) {
            return ours.to_string();
        }
        by_name.get(&key).cloned().unwrap_or(key)
    };

    let games_path = dir.join("3-games.csv");
    let mut writer = csv::Writer::from_path(&games_path)
        .map_err(|e| format!("{}: {e}", games_path.display()))?;
    writer
        .write_record([
            "team",
            "date",
            "site",
            "opp_slug",
            "opp_name",
            "opp_rank",
            "result",
            "score",
            "opp_score",
        ])
        .map_err(|e| e.to_string())?;

    let mut missing: Vec<String> = Vec::new();
    let mut total = 0;

    for team in &teams {
        let aliases = ALIASES
            .iter()
            .filter(|(_, ours)| *ours == team.slug)
            .map(|(sr, _)| sr.to_string());
        let path = [team.slug.clone(), slugify(&team.name)]
            .into_iter()
            .chain(aliases)
            .map(|stem| dir.join(format!("{stem}.csv")))
            .find(|p| p.exists());

        let Some(path) = path else {
            missing.push(format!("{}.csv  ({})", team.slug, team.name));
            continue;
        };

        // Sports Reference schedule columns:
        // 1 date, 5 site ("@" / "N" / ""), 6 opponent, 8 W/L, 9 pts, 10 opp pts
        let mut count = 0;
        for rec in read_records(&path)? {
            if rec.len() < 11 || rec.get(0) == Some("Rk") {
                continue;
            }
            let field = |i: usize| rec.get(i).unwrap_or("").trim();
            let (opp_rank, opp_name) = split_rank(field(6));
            if opp_name.is_empty() {
                continue;
            }

            writer
                .write_record([
                    team.slug.as_str(),
                    field(1),
                    field(5),
                    &resolve(&opp_name),
                    &opp_name,
                    &opp_rank.map(|r| r.to_string()).unwrap_or_default(),
                    field(8),
                    field(9),
                    field(10),
                ])
                .map_err(|e| e.to_string())?;
            count += 1;
        }

        if count == 0 {
            eprintln!("warning: {} had no game rows", path.display());
        }
        println!("{:<24} {count:>2} games  <- {}", team.name, path.display());
        total += count;
    }
    writer.flush().map_err(|e| e.to_string())?;

    println!("\nWrote {total} games to {}", games_path.display());
    if !missing.is_empty() {
        println!("\nNo schedule file found for {} team(s):", missing.len());
        for m in &missing {
            println!("  {m}");
        }
    }
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
