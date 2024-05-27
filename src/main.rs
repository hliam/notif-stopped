use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;

// TODO: add color to errors (to match clap) and `--no-color` option.

fn main() -> ExitCode {
    match run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Run the program.
fn run() -> Result<(), String> {
    let cli = Cli::parse();
    cli.enforce_invariants()?;
    let interval = std::time::Duration::from_secs(cli.interval);
    // This'll be `None` if it's a dry run and `Some` if it isn't.
    let maybe_url = (!cli.dry_run).then(get_webhook_url).transpose()?;

    if !block_while_process_running(&cli.process_name, interval) {
        return Err(format!("process isn't running: {}", cli.process_name));
    }

    // The process has stopped at this point.
    if let Some(url) = maybe_url {
        println!(
            "Process stopped, sending notification: {}",
            cli.process_name
        );
        minreq::post(url)
            .send()
            .map_err(|e| format!("http request failed: {e}"))?;
    } else {
        println!("Process stopped: {}", cli.process_name);
    }

    Ok(())
}

/// Send a notification to your phone when a program stops running.
///
/// The url for the webhook can be set with the `NOTIF_URL` environment variable (and can be set in
/// a .env file that's either in the same directory as the exe or in the current working directory).
///
/// The program must be currently running. This requires an app (on your phone) that will send a
/// notification when a webhook is POSTed to (such as Pushcut). This can also be used for other,
/// non-notification webhooks. Note that the process name is needed, not the window title.
#[derive(Parser)]
struct Cli {
    /// Name of the process to listen for (not the window title)
    process_name: String,
    // secs
    /// How often to check if it's running (in seconds)
    #[arg(short, long, default_value_t = 10)]
    interval: u64,
    /// Don't send the notification, just print the stopped message & exit
    #[arg(short, long)]
    dry_run: bool,
}

impl Cli {
    fn enforce_invariants(&self) -> Result<(), String> {
        if self.process_name.is_empty() {
            return Err("process name can't be empty".to_owned());
        }

        if self.interval < 1 {
            Err("interval is too short (must be at least 1 second)".to_owned())
        } else {
            Ok(())
        }
    }
}

/// Gets the webhook url from a `NOTIF_URL` environment variable (or .env file).
///
/// This does basic validation that the url is actually a url. Errors reflect io errors, .env
/// parsing errors, and invalid pushcut paths.
fn get_webhook_url() -> Result<String, String> {
    let cur_exe = std::env::current_exe().map_err(|e| format!("failed to get current exe: {e}"))?;
    let exe_dir = cur_exe
        .parent()
        .ok_or_else(|| "failed to get current exe's parent directory".to_owned())?;

    match dotenvy::from_path(exe_dir.join(".env")) {
        Ok(_) => (),
        Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("failed read .env file in exe directory: {e}")),
    }
    match dotenvy::dotenv() {
        Ok(_) => (),
        Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => {
            return Err(format!(
                "failed read .env file in current working directory: {e}"
            ))
        }
    }

    match std::env::var("NOTIF_URL").map_err(|e| e.to_string())? {
        p if p.is_empty() => {
            Err("'NOTIF_URL' environment variable needs to be set (to the webhook url)".to_owned())
        }
        p if !p.starts_with("http") => Err("`NOTIF_URL` must be a url".to_owned()),
        p => Ok(p),
    }
}

/// Blocks while a process with a specified name is running. Returns `false` if the process wasn't
/// running in the first place.
///
/// Whether or not the process is running with be regularly every `check_interval` duration.
fn block_while_process_running(process_name: &str, check_interval: Duration) -> bool {
    let mut s = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::everything()),
    );
    let pid = match s.processes_by_exact_name(process_name).next() {
        Some(process) => process.pid(),
        None => return false,
    };

    while s.process(pid).is_some() {
        std::thread::sleep(check_interval);
        s.refresh_pids(&[pid]);
    }

    true
}
