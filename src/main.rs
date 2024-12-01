use colored::*;
use std::{sync::Arc, time::Duration};

use futures::{
    io::BufReader, AsyncBufReadExt as _, AsyncReadExt as _, SinkExt as _, StreamExt as _,
};
use gpui::{App, AppContext, AsyncAppContext};
use http_client::{AsyncBody, HttpClient, Request};
use jupyter_protocol::ExecuteRequest;
use jupyter_websocket_client::{KernelLaunchRequest, RemoteServer};
use mybinder::parse_binder_build_response;

use reqwest_client::ReqwestClient;

fn app_main(cx: &mut AppContext) {
    let http_client = Arc::new(
        ReqwestClient::proxy_and_user_agent(None, "github.com/runtimed/smoke")
            .expect("could not start HTTP client"),
    );
    cx.set_http_client(http_client.clone());

    cx.spawn(|cx: AsyncAppContext| async move {
        let response = http_client
            .get(
                "https://mybinder.org/build/gh/binder-examples/conda_environment/HEAD",
                AsyncBody::empty(),
                true,
            )
            .await?;

        println!(
            "{}",
            "🔗 Connected to Binder, processing response...\n"
                .bold()
                .bright_green()
                .dimmed()
        );

        let reader = BufReader::new(response.into_body());
        let mut stream = reader.lines().boxed();

        let mut remote_server: Option<RemoteServer> = None;

        while let Some(line_result) = stream.next().await {
            match line_result {
                Ok(line) => {
                    if line.is_empty() || line == ":keepalive" || line.starts_with(":") {
                        continue;
                    }
                    match parse_binder_build_response(&line) {
                        Ok(build_response) => match build_response.phase {
                            mybinder::Phase::Ready { url, token, .. } => {
                                println!(
                                    "\n📘 {}\n",
                                    "Notebook server online!".bright_blue().bold(),
                                );

                                println!("🔗 URL: {}", url.bold().underline());

                                remote_server = Some(RemoteServer {
                                    base_url: url,
                                    token,
                                });

                                break;
                            }
                            mybinder::Phase::Failed { message } => {
                                println!(
                                    "😲 Binder failed: {:?}",
                                    message.as_ref().map(|m| m.bright_red())
                                );
                                return cx.update(|cx| {
                                    cx.quit();
                                });
                            }
                            mybinder::Phase::Built { message, .. } => {
                                println!(
                                    "📦 {}",
                                    message.as_ref().unwrap_or(&"".to_string()).bright_cyan()
                                );

                                // if let Some(image_name) = image_name {
                                //     println!("📦 Binder image name: {}", image_name.green());
                                // }
                            }
                            mybinder::Phase::Launching { message } => {
                                println!(
                                    "{}",
                                    message.as_ref().unwrap_or(&"".to_string()).dimmed()
                                );
                            }
                            mybinder::Phase::Waiting { message } => {
                                println!(
                                    "🕒 Binder waiting: {}",
                                    message.as_ref().unwrap_or(&"".to_string()).bright_green()
                                );
                            }
                            mybinder::Phase::Fetching { message } => {
                                println!(
                                    "📥 Binder fetching: {}",
                                    message.as_ref().unwrap_or(&"".to_string()).bright_green()
                                );
                            }

                            mybinder::Phase::Building { message } => {
                                println!(
                                    "📦 Binder building: {}",
                                    message.as_ref().unwrap_or(&"".to_string()).bright_green()
                                );
                            }
                            mybinder::Phase::Unknown { message } => {
                                println!(
                                    "🤷 Binder unknown: {}",
                                    message.as_ref().unwrap_or(&"".to_string()).bright_green()
                                );
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

        let remote_server =
            remote_server.ok_or(anyhow::anyhow!("Binder did not start successfully"))?;

        println!();

        let kernel_launch_request = KernelLaunchRequest {
            name: "python".to_string(),
            path: None,
        };

        println!("{}", "🌽 Launching kernel".bright_yellow());

        let kernel_launch_request = serde_json::to_string(&kernel_launch_request)?;

        let request = Request::builder()
            .method("POST")
            .uri(&remote_server.api_url("/kernels"))
            .header("Authorization", format!("token {}", remote_server.token))
            .body(AsyncBody::from(kernel_launch_request))?;

        let response = http_client.send(request).await?;

        if !response.status().is_success() {
            let mut body = String::new();
            response.into_body().read_to_string(&mut body).await?;
            anyhow::bail!("Failed to launch kernel: {}", body);
        }

        let mut body = String::new();
        response.into_body().read_to_string(&mut body).await?;

        let response: jupyter_websocket_client::Kernel = serde_json::from_str(&body)?;

        let kernel_id = response.id;

        println!("{}", "🌽 Kernel launched".bright_yellow());

        let (ws, _ws_response) = remote_server.connect_to_kernel(&kernel_id).await?;

        let (mut w, mut r) = ws.split();

        cx.spawn(|cx| async move {
            while let Some(message) = r.next().await {
                match message {
                    Ok(message) => {
                        // Normally would update a model here
                        match message.content {
                            jupyter_protocol::JupyterMessageContent::ExecuteResult(
                                execute_result,
                            ) => {
                                let content = execute_result.data.content;
                                let media = content.first();

                                let result = match media {
                                    Some(jupyter_protocol::MediaType::Plain(text)) => text,
                                    _ => "🤷 Unknown media type",
                                };

                                assert_eq!(result, "4");

                                println!("✅ Executed code successfully");

                                return cx.update(|cx| {
                                    cx.quit();
                                });
                            }
                            jupyter_protocol::JupyterMessageContent::Status(status) => {
                                match status.execution_state {
                                    jupyter_protocol::ExecutionState::Idle => {
                                        println!("🌽 {}", "idle".dimmed());
                                    }
                                    jupyter_protocol::ExecutionState::Busy => {
                                        println!("🌽 {}", "busy".bright_magenta().dimmed());
                                    }
                                }
                            }
                            _ => {
                                // dbg!(&message.content);
                            }
                        }
                    }
                    Err(e) => eprintln!("Error reading message: {:?}", e),
                }
            }
            anyhow::Ok(())
        })
        .detach();

        println!("{}", "🌽 Executing code".bright_yellow());

        w.send(
            ExecuteRequest {
                code: "2 + 2".to_string(),
                store_history: false,
                allow_stdin: false,
                stop_on_error: false,
                silent: false,
                user_expressions: None,
            }
            .into(),
        )
        .await?;

        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;

        w.close().await?;

        anyhow::bail!("Failed to execute code");

        #[allow(unreachable_code)]
        Ok(())
    })
    .detach_and_log_err(cx);
}

fn main() {
    App::new().run(app_main);
}
