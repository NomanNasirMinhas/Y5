use crate::{
    engine::{Engine, Request},
    model::{CaptureInfo, Network},
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{prelude::*, widgets::*};
use std::{
    collections::{HashSet, VecDeque},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const TABS: [&str; 7] = [
    "Overview",
    "Networks",
    "Targets",
    "Captures",
    "Interfaces",
    "Tools",
    "Logs",
];
const ACTIONS: [&str; 8] = [
    "handshake",
    "pmkid",
    "wps",
    "deauth",
    "aircrack",
    "hashcat",
    "export",
    "devices",
];
const TOOLS: [&str; 9] = [
    "wifite",
    "kismet",
    "hashcat",
    "tshark",
    "tcpdump",
    "aircrack-ng",
    "hcxpcapngtool",
    "reaver",
    "bully",
];
pub struct App {
    tab: usize,
    row: usize,
    networks: Vec<Network>,
    selected: HashSet<String>,
    captures: Vec<CaptureInfo>,
    interfaces: Vec<crate::model::WirelessInterface>,
    logs: VecDeque<String>,
    checks: Vec<String>,
    iface: usize,
    action: usize,
    input: Option<(usize, String)>,
    form: Request,
    field: usize,
    message: String,
    filter: String,
    confirm: bool,
    queue: VecDeque<Request>,
    quitting: bool,
    tool_args: String,
    clients: Vec<crate::model::Client>,
    show_clients: bool,
}
impl App {
    pub fn new() -> Self {
        Self {
            tab: 0,
            row: 0,
            networks: vec![],
            selected: HashSet::new(),
            captures: vec![],
            interfaces: vec![],
            logs: VecDeque::new(),
            checks: vec!["Checking tools and interfaces…".into()],
            iface: 0,
            action: 0,
            input: None,
            form: Request::default(),
            field: 0,
            message: "Choose an interface, then press s to scan. ? shows help.".into(),
            filter: String::new(),
            confirm: false,
            queue: VecDeque::new(),
            quitting: false,
            tool_args: "[]".into(),
            clients: vec![],
            show_clients: false,
        }
    }
    fn visible(&self) -> Vec<&Network> {
        self.networks
            .iter()
            .filter(|n| {
                format!("{} {}", n.essid, n.bssid)
                    .to_lowercase()
                    .contains(&self.filter.to_lowercase())
            })
            .collect()
    }
    fn selected_iface(&self) -> String {
        self.interfaces
            .get(self.iface)
            .map(|i| i.name.clone())
            .unwrap_or_default()
    }
    fn fields(&self) -> Vec<(&str, String)> {
        vec![
            ("Capture path", self.form.capture.clone()),
            ("Wordlist path", self.form.wordlist.clone()),
            ("Rules path (optional)", self.form.rules.clone()),
            ("Mask (optional)", self.form.mask.clone()),
            ("Packet count (0 = continuous)", self.form.count.to_string()),
            ("Client MAC (optional)", self.form.client.clone()),
            ("Aircrack mode (auto/wpa/wep)", self.form.mode.clone()),
            (
                "Hashcat increment (true/false)",
                self.form.increment.to_string(),
            ),
        ]
    }
    fn consume(&mut self, engine: &Engine) -> bool {
        let mut inbox = engine.bus.lock().unwrap();
        let mut changed = false;
        if inbox.failed {
            self.queue.clear();
        }
        if let Some(s) = inbox.snapshot.take() {
            self.networks = s.networks;
            self.clients = s.clients;
            self.message = s.message;
            changed = true;
        }
        if let Some(s) = inbox.status.take() {
            self.message = format!(
                "{} {:.1}% {} {}",
                s.mode,
                s.progress * 100.0,
                s.speed.unwrap_or_default(),
                s.message
            );
            if let Some(key) = s.key {
                self.message.push_str(&format!(" | Recovered: {key}"));
            }
            changed = true;
        }
        while let Some(log) = inbox.logs.pop_front() {
            if self.logs.len() == 512 {
                self.logs.pop_front();
            }
            self.logs
                .push_back(format!("{} {}", log.level.to_uppercase(), log.message));
            changed = true;
        }
        changed
    }
    fn requests(&self) -> Result<VecDeque<Request>, String> {
        let mut r = self.form.clone();
        r.action = ACTIONS[self.action].into();
        r.interface = self.selected_iface();
        if self.action < 4 {
            let targets: Vec<_> = self
                .networks
                .iter()
                .filter(|n| self.selected.contains(&n.bssid))
                .collect();
            if targets.is_empty() {
                return Err("Select a discovered network first (Space on Networks)".into());
            }
            Ok(targets
                .into_iter()
                .map(|n| {
                    let mut r = r.clone();
                    r.bssid = n.bssid.clone();
                    r.channel = n.channel;
                    r
                })
                .collect())
        } else {
            Ok(VecDeque::from([r]))
        }
    }
}

pub fn run(engine: &mut Engine, stop: Arc<AtomicBool>) -> io::Result<()> {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            let _ = crossterm::execute!(io::stdout(), crossterm::cursor::Show);
        }
    }
    let _restore = Restore;
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let refresh_root = engine.root.clone();
    let refresh = |tx: std::sync::mpsc::SyncSender<_>| {
        let root = refresh_root.clone();
        std::thread::spawn(move || {
            let checks = crate::preflight::check();
            let interfaces = crate::wifi::list_interfaces().unwrap_or_default();
            let captures = crate::replay::list(&root.join("scans")).unwrap_or_default();
            let _ = tx.send((checks, interfaces, captures));
        });
    };
    refresh(tx.clone());
    let mut refreshing = true;
    let mut dirty = true;
    let mut was_busy = false;
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    loop {
        if let Ok((checks, interfaces, captures)) = rx.try_recv() {
            app.checks = checks;
            app.interfaces = interfaces;
            app.captures = captures;
            dirty = true;
            refreshing = false;
        }
        dirty |= app.consume(engine);
        let busy = engine.busy();
        if was_busy != busy {
            dirty = true;
            if !busy {
                app.captures = crate::replay::list(&engine.root.join("scans")).unwrap_or_default();
                if !refreshing {
                    refresh(tx.clone());
                    refreshing = true;
                }
            }
        }
        was_busy = busy;
        if stop.load(Ordering::Relaxed) && !app.quitting {
            app.quitting = true;
            app.queue.clear();
            engine.stop();
            dirty = true;
        }
        if app.quitting && !busy {
            break;
        }
        if !busy && !app.quitting {
            if let Some(request) = app.queue.pop_front() {
                if let Err(e) = engine.start(request) {
                    app.message = e;
                    app.queue.clear();
                }
                dirty = true;
            }
        }
        if dirty && last_draw.elapsed() >= Duration::from_millis(200) {
            terminal.draw(|f| draw(f, &app, engine))?;
            dirty = false;
            last_draw = Instant::now();
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let ev = event::read()?;
        if matches!(ev, Event::Resize(..)) {
            dirty = true;
            continue;
        }
        let Event::Key(key) = ev else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        dirty = true;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            app.quitting = true;
            app.queue.clear();
            engine.stop();
            continue;
        }
        if let Some((field, mut text)) = app.input.take() {
            match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => match field {
                    0 => app.form.capture = text,
                    1 => app.form.wordlist = text,
                    2 => app.form.rules = text,
                    3 => app.form.mask = text,
                    4 => match text.parse() {
                        Ok(v) => app.form.count = v,
                        Err(_) => app.message = "Invalid packet count".into(),
                    },
                    98 => app.tool_args = text,
                    5 => app.form.client = text,
                    6 => app.form.mode = text,
                    7 => match text.parse() {
                        Ok(value) => app.form.increment = value,
                        Err(_) => app.message = "Enter true or false".into(),
                    },
                    _ => app.filter = text,
                },
                KeyCode::Backspace => {
                    text.pop();
                    app.input = Some((field, text));
                }
                KeyCode::Char(c) if text.len() < 4096 => {
                    text.push(c);
                    app.input = Some((field, text));
                }
                _ => app.input = Some((field, text)),
            }
            continue;
        }
        if app.confirm {
            app.confirm = false;
            if key.code == KeyCode::Char('y') {
                match app.requests() {
                    Ok(q) => app.queue = q,
                    Err(e) => app.message = e,
                }
            }
            continue;
        }
        match key.code {
            KeyCode::Char('q')=>{app.quitting=true;app.queue.clear();engine.stop();},
            KeyCode::Char('x')=>{app.queue.clear();engine.stop();app.message="Stopping; cleanup continues in background".into();},
            KeyCode::Tab=>{app.tab=(app.tab+1)%TABS.len();app.row=0;},
            KeyCode::BackTab=>{app.tab=(app.tab+TABS.len()-1)%TABS.len();app.row=0;},
            KeyCode::Char(c @ '1'..='7')=>{app.tab=(c as u8-b'1')as usize;app.row=0;},
            KeyCode::Down=>{if app.tab==2{app.field=(app.field+1)%8;}else{let max=match app.tab {1=>app.visible().len(),3=>app.captures.len(),4=>app.interfaces.len(),5=>TOOLS.len(),6=>app.logs.len(),_=>app.checks.len()};app.row=(app.row+1).min(max.saturating_sub(1));}},
            KeyCode::Up=>{if app.tab==2{app.field=(app.field+7)%8;}else{app.row=app.row.saturating_sub(1);}},
            KeyCode::Left if app.tab==2=>app.action=(app.action+ACTIONS.len()-1)%ACTIONS.len(),
            KeyCode::Right if app.tab==2=>app.action=(app.action+1)%ACTIONS.len(),
            KeyCode::Char('/')=>app.input=Some((99,app.filter.clone())),
            KeyCode::Char(' ') if app.tab==1=>{if let Some(n)=app.visible().get(app.row).map(|n|n.bssid.clone()){if !app.selected.remove(&n){app.selected.insert(n);}}},
            KeyCode::Char('c') if app.tab==1=>app.selected.clear(),
            KeyCode::Char('v') if app.tab==1=>{app.show_clients = !app.show_clients;app.row=0;},
            KeyCode::Enter if app.tab==1=>{if let Some(n)=app.visible().get(app.row).map(|n|n.bssid.clone()){app.selected.clear();app.selected.insert(n);app.tab=2;}},
            KeyCode::Enter if app.tab==2=>{let fields=app.fields();app.input=Some((app.field,fields[app.field].1.clone()));},
            KeyCode::Enter if app.tab==3=>{if let Some(c)=app.captures.get(app.row){app.form.capture=c.path.clone();app.tab=2;app.action=5;}},
            KeyCode::Enter if app.tab==4=>{if app.row<app.interfaces.len(){app.iface=app.row;app.message=format!("Selected {}",app.selected_iface());}},
            KeyCode::Enter if app.tab==5=>app.input=Some((98,app.tool_args.clone())),
            KeyCode::Char('g') if app.tab==5=>{
                if busy || !app.queue.is_empty() {app.message="Stop jobs before handing over the terminal".into();continue;}
                let extra:Vec<String>=match serde_json::from_str(&app.tool_args){Ok(v)=>v,Err(e)=>{app.message=format!("Arguments must be a JSON string array: {e}");continue;}};
                let tool=TOOLS[app.row.min(TOOLS.len()-1)];
                let mut args=vec![];
                let iface=app.selected_iface();
                if !iface.is_empty() && matches!(tool,"wifite"|"kismet"|"tshark"|"tcpdump"|"reaver"|"bully") { args.extend(["--interface".into(),iface]); }
                let index=args.len();args.extend(["tool".into(),tool.into(),"--".into()]);args.extend(extra);
                crossterm::terminal::disable_raw_mode()?;
                crossterm::execute!(io::stdout(),crossterm::terminal::LeaveAlternateScreen)?;
                let result=crate::handoff(engine,&args,index,stop.clone());
                stop.store(false,Ordering::Relaxed);
                crossterm::execute!(io::stdout(),crossterm::terminal::EnterAlternateScreen)?;
                crossterm::terminal::enable_raw_mode()?;terminal.clear()?;
                app.message=match result {Ok(())=>format!("{tool} completed; terminal returned to Y5"),Err(e)=>e.to_string()};
            },
            KeyCode::Char('s')=>{let r=Request{action:"scan".into(),interface:app.selected_iface(),..Request::default()};if let Err(e)=engine.start(r){app.message=e;}else{app.tab=1;}},
            KeyCode::Char('R')=>{if let Err(e)=engine.start(Request{action:"recover".into(),..Request::default()}){app.message=e;}},
            KeyCode::Char('r') if !refreshing=>{refresh(tx.clone());refreshing=true;app.message="Refreshing tool and interface checks…".into();},
            KeyCode::Char('g') if app.tab==2=>{if busy{app.message="Stop the active job first".into();}else{match app.requests(){Ok(_)=>app.confirm=true,Err(e)=>app.message=e}}},
            KeyCode::Char('?')=>app.message="Tab/1–7: workspace | ↑↓: row | Space: multi-select | Enter: select/edit | ←→: workflow | g: run | s: scan | x: stop | R: recover | q: exit".into(),
            _=>{},
        }
    }
    Ok(())
}

pub fn draw(f: &mut Frame, app: &App, engine: &Engine) {
    let area = f.area();
    if area.width < 30 || area.height < 10 {
        f.render_widget(
            Paragraph::new("Y5: resize to at least 30x10. q exits; x stops jobs."),
            area,
        );
        return;
    }
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(area);
    let job = engine
        .job
        .as_ref()
        .map(|j| j.name.as_str())
        .unwrap_or("idle");
    f.render_widget(
        Paragraph::new(format!(
            " Y5  /  AIRWAVE CONSOLE     {}     {}     queued: {}",
            app.selected_iface(),
            job,
            app.queue.len()
        ))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::bordered()),
        layout[0],
    );
    f.render_widget(
        Tabs::new(if area.width < 80 {
            vec![TABS[app.tab]]
        } else {
            TABS.to_vec()
        })
        .select(if area.width < 80 { 0 } else { app.tab })
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        layout[1],
    );
    let body = layout[2];
    match app.tab {
        0 => f.render_widget(
            Paragraph::new(app.checks.join("\n"))
                .block(Block::bordered().title(" Startup checks / optional tools are lazy-loaded "))
                .scroll((app.row.min(u16::MAX as usize) as u16, 0)),
            body,
        ),
        1 if app.show_clients => {
            let text = app
                .clients
                .iter()
                .map(|c| {
                    format!(
                        "{}  AP {}  {} dBm  {} packets\nProbes: {}",
                        c.mac, c.bssid, c.power, c.packets, c.probed_essids
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            f.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .scroll((app.row.min(u16::MAX as usize) as u16, 0))
                    .block(Block::bordered().title(" Associated clients / v returns to networks ")),
                body,
            );
        }
        1 => {
            let visible = app.visible();
            if body.width < 76 {
                let items = visible.iter().map(|n| {
                    ListItem::new(format!(
                        "{} {}  {} dBm\n   {}  ch {} {}",
                        if app.selected.contains(&n.bssid) {
                            "[x]"
                        } else {
                            "[ ]"
                        },
                        n.essid,
                        n.power,
                        n.bssid,
                        n.channel,
                        n.privacy
                    ))
                });
                let mut state = ListState::default()
                    .with_selected(Some(app.row.min(visible.len().saturating_sub(1))));
                f.render_stateful_widget(
                    List::new(items)
                        .highlight_style(Style::default().bg(Color::DarkGray))
                        .block(Block::bordered().title(" Networks / Space selects ")),
                    body,
                    &mut state,
                );
            } else {
                let rows = visible.iter().map(|n| {
                    Row::new(vec![
                        if app.selected.contains(&n.bssid) {
                            "[x]".into()
                        } else {
                            "[ ]".into()
                        },
                        n.essid.clone(),
                        n.bssid.clone(),
                        n.channel.to_string(),
                        n.power.to_string(),
                        n.privacy.clone(),
                    ])
                });
                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(4),
                        Constraint::Min(10),
                        Constraint::Length(18),
                        Constraint::Length(4),
                        Constraint::Length(6),
                        Constraint::Length(12),
                    ],
                )
                .header(
                    Row::new(["", "SSID", "BSSID", "CH", "dBm", "Security"])
                        .style(Style::default().fg(Color::Cyan)),
                )
                .row_highlight_style(Style::default().bg(Color::DarkGray))
                .block(Block::bordered().title(format!(
                    " Networks {} / selected {} / filter {} ",
                    visible.len(),
                    app.selected.len(),
                    app.filter
                )));
                let mut state = TableState::default()
                    .with_selected(Some(app.row.min(visible.len().saturating_sub(1))));
                f.render_stateful_widget(table, body, &mut state);
            }
        }
        2 => {
            let mut lines = vec![
                format!(
                    "Workflow: < {} >    [g] Review and run",
                    ACTIONS[app.action]
                ),
                format!(
                    "{} selected target(s); radio jobs run sequentially",
                    app.selected.len()
                ),
                String::new(),
            ];
            for (i, (label, value)) in app.fields().iter().enumerate() {
                lines.push(format!(
                    "{} {label}: {value}",
                    if i == app.field { ">" } else { " " }
                ));
            }
            lines.push("\nEnter edits field. Captures tab selects a file. Hashcat mask overrides wordlist mode.".into());
            f.render_widget(
                Paragraph::new(lines.join("\n"))
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title(" Workflow configuration ")),
                body,
            );
        }
        3 => {
            let items = app
                .captures
                .iter()
                .map(|c| ListItem::new(format!("{}  ({} KiB)", c.name, c.bytes / 1024)));
            let mut state = ListState::default()
                .with_selected(Some(app.row.min(app.captures.len().saturating_sub(1))));
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(Color::DarkGray))
                    .block(Block::bordered().title(" Captures / Enter: use for recovery ")),
                body,
                &mut state,
            );
        }
        4 => {
            let items = app
                .interfaces
                .iter()
                .map(|i| ListItem::new(format!("{} {} {}", i.name, i.interface_type, i.mac)));
            let mut state = ListState::default()
                .with_selected(Some(app.row.min(app.interfaces.len().saturating_sub(1))));
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(Color::DarkGray))
                    .block(
                        Block::bordered()
                            .title(" Interfaces / Enter: select / R: recover recorded adapter "),
                    ),
                body,
                &mut state,
            );
        }
        5 => {
            let mut text = TOOLS
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{} {t}", if i == app.row { ">" } else { " " }))
                .collect::<Vec<_>>()
                .join("\n");
            text.push_str(&format!("\n\nArguments: {}\nEnter edits JSON array, e.g. [\"--help\"]. g launches selected tool.\nThe tool owns the terminal until exit; Y5 then resumes.\nPass its own interface flags explicitly. Radio tools journal the selected adapter.\nKismet uses its own console/API; its panels are not duplicated here.",app.tool_args));
            f.render_widget(
                Paragraph::new(text).wrap(Wrap { trim: false }).block(
                    Block::bordered()
                        .title(" Optional integrations / full upstream terminal interface "),
                ),
                body,
            );
        }
        _ => {
            let count = body.height.saturating_sub(2) as usize;
            let lines = app
                .logs
                .iter()
                .rev()
                .skip(app.row)
                .take(count)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::bordered().title(" Activity / bounded to 512 lines / ↑↓ scroll "),
                ),
                body,
            );
        }
    }
    let message = if app.quitting {
        "Exiting: stopping workers and restoring interface…"
    } else {
        &app.message
    };
    f.render_widget(
        Paragraph::new(message)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" s scan · x stop · R recover · ? help · q exit ")),
        layout[3],
    );
    if app.confirm || app.input.is_some() {
        let popup = Rect::new(
            area.x + area.width / 10,
            area.y + area.height / 3,
            area.width * 8 / 10,
            5.min(area.height),
        );
        f.render_widget(Clear, popup);
        let text = if let Some((_, s)) = &app.input {
            format!("{s}\nEnter saves • Esc cancels")
        } else {
            format!("Run {} on {} target(s)? Authorized networks only.\n[y] start   [any other key] cancel",ACTIONS[app.action],app.selected.len())
        };
        f.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(" Confirm / edit ")),
            popup,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_required_for_radio_workflows() {
        let app = App::new();
        assert!(app.requests().is_err());
    }
    #[test]
    fn worker_errors_clear_remaining_queue() {
        let root = std::env::temp_dir().join(format!("y5-queue-test-{}", std::process::id()));
        let engine = Engine::new(root.clone()).unwrap();
        let mut app = App::new();
        app.queue.push_back(Request::default());
        crate::events::report(&engine.bus, "error", "worker failed");
        app.consume(&engine);
        assert!(app.queue.is_empty());
        drop(engine);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn all_screens_render_at_small_and_large_terminal_sizes() {
        let root = std::env::temp_dir().join(format!("y5-ui-test-{}", std::process::id()));
        let engine = Engine::new(root.clone()).unwrap();
        let mut app = App::new();
        app.networks = crate::capture::parse_networks(
            "02:00:00:00:00:01, a, b, 6, 54, WPA2, CCMP, PSK, -48, 12, 3, 0, 0, Lab-router,\n",
        );
        app.selected.insert("02:00:00:00:00:01".into());
        assert_eq!(app.requests().unwrap().len(), 1);
        for (w, h) in [(20, 8), (40, 16), (80, 24), (120, 36)] {
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            for tab in 0..TABS.len() {
                app.tab = tab;
                terminal.draw(|f| draw(f, &app, &engine)).unwrap();
            }
            app.tab = 1;
            terminal.draw(|f| draw(f, &app, &engine)).unwrap();
            if w >= 40 {
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(text.contains("Lab-router"));
                assert!(text.contains("[x]"));
            }
            app.confirm = true;
            terminal.draw(|f| draw(f, &app, &engine)).unwrap();
            app.confirm = false;
            if w == 120 {
                app.tab = 1;
                terminal.draw(|f| draw(f, &app, &engine)).unwrap();
                let text = terminal
                    .backend()
                    .buffer()
                    .content
                    .chunks(w as usize)
                    .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n");
                std::fs::create_dir_all("target/ui").unwrap();
                std::fs::write("target/ui/networks.txt", text).unwrap();
            }
        }
        drop(engine);
        std::fs::remove_dir_all(root).unwrap();
    }
}
