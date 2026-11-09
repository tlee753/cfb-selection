use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::theme::Palette;
use iced::{Background, Border, Color, Element, Font, Length, Theme};
use std::collections::HashMap;

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

// ───────────────────────── Embedded data ─────────────────────────
// Produced by `cargo run --features fetch --bin fetch_data`, baked into the binary.

const AP_CSV: &str = include_str!("../data/ap.csv");
const CFP_CSV: &str = include_str!("../data/cfp.csv");
const GAMES_CSV: &str = include_str!("../data/games.csv");

fn read_csv(data: &str) -> Vec<csv::StringRecord> {
    csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(data.as_bytes())
        .records()
        .filter_map(Result::ok)
        .collect()
}

fn load_schedules() -> HashMap<String, Vec<Game>> {
    // games.csv: team,date,site,opp_slug,opp_name,opp_rank,result,score,opp_score
    let mut schedules: HashMap<String, Vec<Game>> = HashMap::new();
    for rec in read_csv(GAMES_CSV) {
        let field = |i: usize| rec.get(i).unwrap_or("").trim();
        let game = Game {
            date: field(1).to_string(),
            home: field(2) != "@",
            opp_slug: field(3).to_string(),
            opp_name: field(4).to_string(),
            opp_rank: field(5).parse().ok(),
            outcome: match field(6) {
                "W" => Outcome::Won,
                "L" => Outcome::Lost,
                _ => Outcome::Upcoming,
            },
            score: field(7).to_string(),
            opp_score: field(8).to_string(),
        };
        schedules.entry(field(0).to_string()).or_default().push(game);
    }
    schedules
}

/// ap.csv / cfp.csv: rank,slug,name,record
fn load_teams(csv: &str, schedules: &HashMap<String, Vec<Game>>) -> Vec<Team> {
    let mut teams: Vec<Team> = read_csv(csv)
        .iter()
        .filter_map(|rec| {
            let slug = rec.get(1)?.to_string();
            Some(Team {
                rank: rec.get(0)?.trim().parse().ok()?,
                name: rec.get(2)?.to_string(),
                record: rec.get(3).unwrap_or("").to_string(),
                schedule: schedules.get(&slug).cloned().unwrap_or_default(),
                slug,
            })
        })
        .collect();

    teams.sort_by_key(|t| t.rank);
    teams
}

// ───────────────────────── App ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Poll {
    Ap,
    Cfp,
}

struct App {
    ap: Vec<Team>,
    cfp: Vec<Team>,
    poll: Poll,
    selected: usize,
}

impl Default for App {
    fn default() -> Self {
        let schedules = load_schedules();
        Self {
            ap: load_teams(AP_CSV, &schedules),
            cfp: load_teams(CFP_CSV, &schedules),
            poll: Poll::Ap,
            selected: 0, // #1 team
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    SetPoll(Poll),
}

impl App {
    fn teams(&self) -> &[Team] {
        match self.poll {
            Poll::Ap => &self.ap,
            Poll::Cfp => &self.cfp,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Select(i) => self.selected = i,
            Message::SetPoll(poll) => {
                self.poll = poll;
                self.selected = 0;
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let teams = self.teams();
        let Some(selected) = teams.get(self.selected) else {
            return container(text("No data yet. Run: cargo run --features fetch --bin fetch_data"))
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
        for (i, team) in teams.iter().enumerate() {
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
                text(&team.record),
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

        let poll_button = |label: &'static str, poll: Poll| {
            let active = self.poll == poll;
            button(text(label).width(Length::Fill).align_x(iced::alignment::Horizontal::Center))
                .width(Length::Fill)
                .padding([8, 10])
                .on_press(Message::SetPoll(poll))
                .style(move |_theme, status| team_button_style(None, active, status))
        };
        let toggle = row![poll_button("AP", Poll::Ap), poll_button("CFP", Poll::Cfp)]
            .spacing(4)
            .padding([8, 8]);

        let sidebar = container(column![toggle, scrollable(rankings).height(Length::Fill)])
            .width(320)
            .height(Length::Fill);

        // ── Right: selected team's schedule ──
        let header = column![
            text(format!("#{} {}", selected.rank, selected.name)).size(28),
            text(&selected.record).size(16),
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

fn black_theme() -> Theme {
    Theme::custom(
        "Black",
        Palette {
            background: Color::BLACK,
            ..Palette::DARK
        },
    )
}

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("CFB Selection")
        .font(include_bytes!("lexend.ttf").as_slice())
        .default_font(Font::with_name("Lexend"))
        .theme(|_: &App| black_theme())
        .window_size((1000.0, 900.0))
        .run()
}