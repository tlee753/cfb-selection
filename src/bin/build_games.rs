//! Builds `data/games.csv` from the schedule CSVs you download by hand.
//!
//!   cargo run --bin build_games            # reads/writes ./data
//!   cargo run --bin build_games -- some/dir
//!
//! Needs, in the data folder:
//!   ap.csv, cfp.csv          the ranked teams (rank,slug,name,record)
//!   <team>.csv               one Sports Reference schedule CSV per team, named after the
//!                            team's slug ("ohio-state.csv") or its name ("usc.csv")
//!
//! Every team listed in ap.csv or cfp.csv is looked up, so a team in both polls is read once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env};

struct Team {
    slug: String,
    name: String,
}

/// "Texas A&M" -> "texas-am", "Miami (FL)" -> "miami-fl", "Ohio State" -> "ohio-state"
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().filter(|c| !matches!(c, '&' | '\'' | '’' | '.')) {
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

fn read_records(path: &Path) -> Result<Vec<csv::StringRecord>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(reader.records().filter_map(Result::ok).collect())
}

fn read_teams(path: &Path) -> Result<Vec<Team>, String> {
    Ok(read_records(path)?
        .iter()
        .filter_map(|r| {
            Some(Team {
                slug: r.get(1)?.trim().to_string(),
                name: r.get(2)?.trim().to_string(),
            })
        })
        .collect())
}

fn run() -> Result<(), String> {
    let dir = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| "data".into());

    // Ranked teams from both polls, de-duplicated
    let mut teams: Vec<Team> = Vec::new();
    for file in ["ap.csv", "cfp.csv"] {
        for team in read_teams(&dir.join(file))? {
            if !teams.iter().any(|t| t.slug == team.slug) {
                teams.push(team);
            }
        }
    }
    if teams.is_empty() {
        return Err(format!("no teams found in {}/ap.csv or cfp.csv", dir.display()));
    }

    // Opponent names in the schedules ("USC", "Ole Miss", "BYU") -> the slugs the app uses.
    let by_name: HashMap<String, String> = teams
        .iter()
        .map(|t| (slugify(&t.name), t.slug.clone()))
        .collect();
    let resolve = |opponent: &str| {
        let key = slugify(opponent);
        by_name.get(&key).cloned().unwrap_or(key)
    };

    let mut writer = csv::Writer::from_path(dir.join("games.csv")).map_err(|e| e.to_string())?;
    writer
        .write_record([
            "team", "date", "site", "opp_slug", "opp_name", "opp_rank", "result", "score",
            "opp_score",
        ])
        .map_err(|e| e.to_string())?;

    let mut missing: Vec<String> = Vec::new();
    let mut total = 0;

    for team in &teams {
        let path = [team.slug.clone(), slugify(&team.name)]
            .iter()
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

    println!("\nWrote {total} games to {}/games.csv", dir.display());
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