use std::sync::Arc;

use futures::{io::BufReader, AsyncBufReadExt as _, StreamExt as _};
use gpui::{App, AppContext, AsyncAppContext};
use http_client::{AsyncBody, HttpClient};
use mybinder::parse_binder_build_response;

use reqwest_client::ReqwestClient;

fn app_main(cx: &mut AppContext) {
    let http_client = Arc::new(
        ReqwestClient::proxy_and_user_agent(None, "github.com/runtimed/smoke")
            .expect("could not start HTTP client"),
    );
    cx.set_http_client(http_client.clone());

    println!("Kicking off binder build");

    cx.spawn(|cx: AsyncAppContext| async move {
        println!("Starting binder build");
        // Connect to binder
        let response = http_client
            .get(
                "https://mybinder.org/build/gh/binder-examples/conda_environment/HEAD",
                AsyncBody::empty(),
                true,
            )
            .await?;

        println!("Connected to Binder, processing response...");

        let reader = BufReader::new(response.into_body());
        let mut stream = reader.lines().boxed();

        while let Some(line_result) = stream.next().await {
            match line_result {
                Ok(line) => {
                    if line.is_empty() || line == ":keepalive" || line.starts_with(":") {
                        continue;
                    }
                    match parse_binder_build_response(&line) {
                        Ok(build_response) => match build_response.phase {
                            mybinder::Phase::Ready { url, token, .. } => {
                                println!("Binder is ready! URL: {}, Token: {}", url, token);
                                break;
                            }
                            mybinder::Phase::Failed { message } => {
                                println!("Binder failed: {:?}", message);
                                break;
                            }
                            _ => {
                                println!("Current phase: {:?}", build_response.phase);
                            }
                        },
                        Err(e) => {
                            println!("Error parsing response: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("Error reading line: {:?}", e);
                }
            }
        }

        // for line in lines {
        // let response = parse_binder_build_response(&line)?;

        // Connect to a kernel on binder

        // Execute code

        // Verify code executed
        //
        //
        cx.update(|cx| {
            cx.quit();
        })
    })
    .detach_and_log_err(cx);
}

fn main() {
    App::new().run(app_main);
}
