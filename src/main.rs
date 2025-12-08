use adw::{gdk, Application, NavigationPage, NavigationSplitView};
use csv::ReaderBuilder;
use gtk::{glib, prelude::*, ApplicationWindow, Label, ListBox, ListBoxRow};
use ordered_hash_map::OrderedHashMap;
use std::fs::File;

#[derive(Clone)]
struct Rated {
    tag: String,
    team: String,
    rank: i8,
    record: String,
    selected: bool,
    won: bool,
    lost: bool,
    schedule: Vec<Game>,
}

#[derive(Clone)]
struct Game {
    tag: String,
    team: String,
    date: String,
    home: bool,
    won: bool,
    lost: bool,
    score: String,
    opp_score: String,
    ranked: bool,
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.tlee753.cfb-selection")
        .build();

    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("style.css"));

        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(build_ui);
    app.run()
}

fn build_map() -> OrderedHashMap<String, Rated> {
    let mut rating_list: OrderedHashMap<String, Rated> = OrderedHashMap::new();

    if let Ok(file) = File::open("src/ap.csv") {
        let mut rating_reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        for result in rating_reader.records() {
            if let Ok(rating_record) = result {
                let team_and_record = &rating_record[3];
                let parts: Vec<&str> = team_and_record.split('(').collect();
                let tag = parts[0]
                    .trim()
                    .replace("&", "")
                    .replace(" ", "-")
                    .to_lowercase();

                let rating_value: Rated = Rated {
                    tag: tag.clone(),
                    team: parts[0].trim().to_string(),
                    rank: rating_record[2]
                        .parse::<i8>()
                        .expect("Rank isn't interpretted as valid integer!"),
                    record: '('.to_string() + parts[parts.len() - 1],
                    selected: false,
                    won: false,
                    lost: false,
                    schedule: parse_schedule(tag.clone()),
                };

                rating_list.insert(tag, rating_value);
            }
        }
    }

    rating_list
}

fn parse_schedule(tag: String) -> Vec<Game> {
    let mut games = Vec::new();

    let csv_name = format!("src/{}.csv", tag);
    // println!("[DEBUG] Team: {:<20} | CSV: {:<20}", tag, csv_name);

    if let Ok(file) = File::open(csv_name) {
        let mut schedule_reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        for result in schedule_reader.records() {
            if let Ok(schedule) = result {
                let game = Game {
                    tag: schedule[6]
                        .to_string()
                        .replace(
                            &[
                                '(', ')', '&', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
                            ][..],
                            "",
                        )
                        .trim()
                        .replace(" ", "-")
                        .to_lowercase(),
                    team: schedule[6].to_string(),
                    date: schedule[1].to_string(),
                    home: !schedule[5].eq("@"),
                    won: schedule[8].eq("W"),
                    lost: schedule[8].eq("L"),
                    score: schedule[9].to_string(),
                    opp_score: schedule[10].to_string(),
                    ranked: schedule[6].contains(&[
                        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
                    ][..]),
                };

                games.push(game);
            }
        }
    }

    games
}

fn build_ui(app: &Application) {
    let rated_list = build_map();
    let rated_list_box = build_rated_list_box(rated_list.clone());
    let sidebar = NavigationPage::new(&rated_list_box, "sidebar title");
    sidebar.add_css_class("sidebar");

    let default_text = Label::new(Some("Please select a team :D"));
    let content = NavigationPage::new(&default_text, "content title");

    let split_view = NavigationSplitView::builder()
        .sidebar(&sidebar)
        .content(&content)
        .build();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("CFB Selection")
        .child(&split_view)
        .build();

    window.set_default_size(1000, 900);
    window.present();

    rated_list_box.connect_row_selected(move |list, row| {
        let mut selected_tag = String::from("ohio-state");

        if let Some(row) = row {
            if let Some(label) = row.child().and_then(|w| w.downcast::<Label>().ok()) {
                selected_tag = label
                    .text()
                    .replace("&", "")
                    .replace(" ", "-")
                    .to_lowercase();
            }
        }

        // println!("[DEBUG] Selected tag: {}", selected_tag);

        let children = list.observe_children();

        let mut other_tag = String::from("ohio-state");
        for i in 0..children.n_items() {
            if let Some(obj) = children.item(i) {
                if let Ok(row) = obj.downcast::<gtk::ListBoxRow>() {
                    row.remove_css_class("won");
                    row.remove_css_class("lost");
                    row.remove_css_class("upcoming");
                    row.remove_css_class("selected");

                    if let Some(label) = row.child().and_then(|w| w.downcast::<Label>().ok()) {
                        other_tag = label
                            .text()
                            .replace("&", "")
                            .replace(" ", "-")
                            .to_lowercase();
                    }

                    for game in rated_list
                        .clone()
                        .get(&selected_tag)
                        .expect("Couldn't find tag in rated list!")
                        .schedule
                        .clone()
                    {
                        // println!("[DEBUG] {} == {}", game.tag, other_tag);

                        if game.tag == other_tag {
                            if game.won {
                                row.add_css_class("won");
                            } else if game.lost {
                                row.add_css_class("lost");
                            } else {
                                row.add_css_class("upcoming");
                            }
                        }
                    }
                    
                    if selected_tag == other_tag {
                        row.add_css_class("selected");
                    }
                }
            }
        }

        let schedule_list_box = build_schedule_list_box(
            rated_list
                .clone()
                .get(&selected_tag)
                .expect("Couldn't find tag in rated list!")
                .team
                .clone(),
            rated_list
                .clone()
                .get(&selected_tag)
                .expect("Couldn't find tag in rated list!")
                .schedule
                .clone(),
        );

        let content = NavigationPage::new(&schedule_list_box, "content title");

        split_view.set_content(Some(&content));
    });
}

fn build_rated_list_box(rated_list: OrderedHashMap<String, Rated>) -> ListBox {
    let rated_list_box = ListBox::new();

    for rated in rated_list {
        let label = Label::builder()
            .label(rated.1.team.clone())
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let row = ListBoxRow::builder()
            .child(&label)
            .activatable(true)
            .build();

        row.add_css_class("rated_row");

        if rated.1.selected {
            row.add_css_class("selected");
        }

        if rated.1.won {
            row.add_css_class("won");
        }

        if rated.1.lost {
            row.add_css_class("lost");
        }

        rated_list_box.append(&row);
    }

    rated_list_box
}

fn build_schedule_list_box(team: String, schedule: Vec<Game>) -> ListBox {
    let schedule_list_box = ListBox::new();

    for game in schedule {
        let game_text;
        if game.score.eq("") {
            if game.home {
                game_text = format!("{} @ {}", game.team, team);
            } else {
                game_text = format!("{} @ {}", team, game.team);
            } 
        } else {
            if game.home {
                game_text = format!("{}        {} @ {}        {}", game.opp_score, game.team, team, game.score);
            } else {
                game_text = format!("{}        {} @ {}        {}", game.score, team, game.team, game.opp_score);
            } 
        }

        let schedule_label = Label::builder()
            .label(game_text)
            .margin_top(20)
            .margin_bottom(20)
            .build();

        let schedule_row = ListBoxRow::builder()
            .child(&schedule_label)
            .activatable(true)
            .build();

        schedule_row.add_css_class("schedule_row");
        
        if game.won {
            schedule_row.add_css_class("won");
        } else if game.lost {
            schedule_row.add_css_class("lost");
        } else if game.ranked {
            schedule_row.add_css_class("upcoming");
        }

        schedule_list_box.append(&schedule_row);
    }

    schedule_list_box
}
