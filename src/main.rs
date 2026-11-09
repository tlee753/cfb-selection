use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::{Background, Border, Color, Element, Length, Theme};
use std::collections::HashMap;
use std::fs::File;

// ───────────────────────── Colors ─────────────────────────

const GREEN: Color = Color::from_rgb(0.18, 0.63, 0.26);
const RED: Color = Color::from_rgb(0.85, 0.21, 0.20);
const BLUE: Color = Color::from_rgb(0.18, 0.51, 0.97);

// ───────────────────────── Data model ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Won,
    Lost,
    Upcoming,
}

#[derive(Debug, Clone)]
struct Game {
    opp_slug: String,
    opp_name: String,
    opp_rank: Option<u8>,
    date: String,
    home: bool,
    outcome: Outcome,
    score: String,
    opp_score: String,
}

#[derive(Debug, Clone)]
struct Team {
    slug: String,
    name: String,
    rank: u8,
    record: String,
    schedule: Vec<Game>,
}

// ───────────────────────── CSV loading ─────────────────────────

/// "(5) Michigan" -> (Some(5), "Michigan"); "Michigan" -> (None, "Michigan")
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

fn slugify(name: &str) -> String {
    name.replace('&', "")
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn read_csv(path: &str) -> Vec<csv::StringRecord> {
    let Ok(file) = File::open(path) else {
        eprintln!("Couldn't open {path}");
        return Vec::new();
    };
    csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file)
        .records()
        .filter_map(Result::ok)
        .collect()
}

fn load_schedule(slug: &str) -> Vec<Game> {
    read_csv(&format!("data/{slug}.csv"))
        .iter()
        .map(|rec| {
            let field = |i: usize| rec.get(i).unwrap_or("").trim().to_string();
            let (opp_rank, opp_name) = split_rank(&field(6));

            Game {
                opp_slug: slugify(&opp_name),
                opp_name,
                opp_rank,
                date: field(1),
                home: field(5) != "@",
                outcome: match field(8).as_str() {
                    "W" => Outcome::Won,
                    "L" => Outcome::Lost,
                    _ => Outcome::Upcoming,
                },
                score: field(9),
                opp_score: field(10),
            }
        })
        .collect()
}

fn load_teams() -> Vec<Team> {
    let mut teams: Vec<Team> = read_csv("data/ap.csv")
        .iter()
        .filter_map(|rec| {
            let rank = rec.get(2)?.trim().parse::<u8>().ok()?;
            let raw = rec.get(3)?;

            // "Ohio State (62) (12-0)" -> name "Ohio State", record "(12-0)"
            let name = raw.split('(').next().unwrap_or(raw).trim().to_string();
            let record = raw
                .rsplit_once('(')
                .map(|(_, r)| format!("({r}"))
                .unwrap_or_default();
            let slug = slugify(&name);

            Some(Team {
                schedule: load_schedule(&slug),
                slug,
                name,
                rank,
                record,
            })
        })
        .collect();

    teams.sort_by_key(|t| t.rank);
    teams
}

// ───────────────────────── App ─────────────────────────

struct App {
    teams: Vec<Team>,
    selected: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            teams: load_teams(),
            selected: 0, // #1 team
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::Select(i) => self.selected = i,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let Some(selected) = self.teams.get(self.selected) else {
            return container(text("No teams found. Check data/ap.csv"))
                .center(Length::Fill)
                .into();
        };

        // opponent slug -> outcome of the selected team's (latest) game vs them
        let vs: HashMap<&str, Outcome> = selected
            .schedule
            .iter()
            .map(|g| (g.opp_slug.as_str(), g.outcome))
            .collect();

        // ── Left: top 25 ──
        let mut rankings = Column::new().spacing(4).padding(8);
        for (i, team) in self.teams.iter().enumerate() {
            let is_selected = i == self.selected;
            let tint = if is_selected {
                None
            } else {
                vs.get(team.slug.as_str()).map(|o| match o {
                    Outcome::Won => GREEN,
                    Outcome::Lost => RED,
                    Outcome::Upcoming => BLUE,
                })
            };

            let label = row![
                text(team.rank.to_string()).width(28),
                text(&team.name).width(Length::Fill),
                text(&team.record).size(13),
            ]
            .spacing(6);

            rankings = rankings.push(
                button(label)
                    .width(Length::Fill)
                    .padding([8, 10])
                    .on_press(Message::Select(i))
                    .style(move |_theme, status| team_button_style(tint, is_selected, status)),
            );
        }

        let sidebar = container(scrollable(rankings)).width(320).height(Length::Fill);

        // ── Right: selected team's schedule ──
        let header = column![
            text(format!("#{} {}", selected.rank, selected.name)).size(28),
            text(&selected.record).size(16),
            text("Win · Loss · Upcoming ranked matchup").size(12),
        ]
        .spacing(4);

        let mut schedule = Column::new().spacing(6);
        for game in &selected.schedule {
            schedule = schedule.push(game_row(game));
        }

        let content = column![header, scrollable(schedule).height(Length::Fill)]
            .spacing(16)
            .padding(20);

        row![sidebar, content.width(Length::Fill)].into()
    }
}

// ───────────────────────── Widgets ─────────────────────────

fn game_row(game: &Game) -> Element<'_, Message> {
    let tint = match game.outcome {
        Outcome::Won => Some(GREEN),
        Outcome::Lost => Some(RED),
        Outcome::Upcoming if game.opp_rank.is_some() => Some(BLUE),
        Outcome::Upcoming => None,
    };

    let opponent = match game.opp_rank {
        Some(r) => format!("#{r} {}", game.opp_name),
        None => game.opp_name.clone(),
    };
    let site = if game.home { "vs" } else { "@" };

    let result = match game.outcome {
        Outcome::Won => format!("W {}–{}", game.score, game.opp_score),
        Outcome::Lost => format!("L {}–{}", game.score, game.opp_score),
        Outcome::Upcoming => String::new(),
    };

    let line = row![
        text(&game.date).width(110),
        text(site).width(24),
        text(opponent).width(Length::Fill),
        text(result),
    ]
    .spacing(8);

    container(line)
        .width(Length::Fill)
        .padding([14, 16])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(match tint {
                Some(c) => Color { a: 0.35, ..c },
                None => Color::from_rgba(1.0, 1.0, 1.0, 0.04),
            })),
            border: Border {
                color: tint.unwrap_or(Color::TRANSPARENT),
                width: if tint.is_some() { 1.0 } else { 0.0 },
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn team_button_style(tint: Option<Color>, selected: bool, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let bump = if hovered { 0.15 } else { 0.0 };

    let (background, border_color, border_width) = if selected {
        (Color::from_rgba(1.0, 1.0, 1.0, 0.18 + bump), Color::WHITE, 2.0)
    } else if let Some(c) = tint {
        (Color { a: 0.35 + bump, ..c }, c, 1.0)
    } else {
        (Color::from_rgba(1.0, 1.0, 1.0, 0.04 + bump), Color::TRANSPARENT, 0.0)
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: Border {
            color: border_color,
            width: border_width,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

// ───────────────────────── Entry point ─────────────────────────

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("CFB Selection")
        .theme(|_: &App| Theme::Dark)
        .window_size((1000.0, 900.0))
        .run()
}