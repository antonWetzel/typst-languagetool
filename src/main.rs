mod convert;
mod output;

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use languagetool_rust::{
    check::{CheckRequest, Data},
    error::Error,
    server::ServerClient,
};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use output::Position;

#[derive(Parser, Debug)]
struct Args {
    /// Document Language. Defaults to auto-detect, but explicit codes ("de-DE", "en-US", ...) enable more checks
    #[clap(short, long, default_value = None)]
    language: Option<String>,

    /// Delay in seconds
    #[clap(short, long, default_value_t = 0.1)]
    delay: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (tx, rx) = std::sync::mpsc::channel();
    let client = ServerClient::new("http://127.0.0.1", "8081");
    if let Some(value) = &args.language {
        client
            .check(
                &CheckRequest::default()
                    .with_language(value.clone())
                    .with_text(String::from("")),
            )
            .await?;
    }

    let mut watcher = new_debouncer(Duration::from_secs_f64(args.delay), None, tx)?;

    watcher
        .watcher()
        .watch(Path::new("."), RecursiveMode::Recursive)?;

    for events in rx {
        for event in events.unwrap() {
            match event.path.extension() {
                Some(ext) if ext == "typ" => {}
                _ => continue,
            }
            handle_file(&client, &args, event.path)
                .await
                .unwrap_or_else(|err| println!("{}", err));
        }
    }

    Ok(())
}

async fn handle_file(client: &ServerClient, args: &Args, file: PathBuf) -> Result<(), Error> {
    let text = fs::read_to_string(&file)?;

    let root = typst_syntax::parse(&text);
    let data = convert::convert(&root);

    println!("START");
    let mut position = Position::new(&text);
    for items in data {
        let req = CheckRequest::default()
            .with_language(match &args.language {
                Some(value) => value.clone(),
                None => "auto".into(),
            })
            .with_data(Data::from_iter(items.0));

        let response = &client.check(&req).await?;
        output::output(&file, &mut position, response, items.1);
    }
    println!("END");
    Ok(())
}
