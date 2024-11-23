use futures::{io::BufReader, AsyncBufReadExt as _, StreamExt as _};
use gpui::{http_client::AsyncBody, App, AppContext, AsyncAppContext};
use mybinder::parse_binder_build_response;

fn app_main(cx: &mut AppContext) {
    let http_client = cx.http_client();

    cx.spawn(|cx: AsyncAppContext| async move {
        // Connect to binder
        let response = http_client
            .get(
                "https://mybinder.org/build/gh/binder-examples/conda_environment/HEAD",
                AsyncBody::empty(),
                true,
            )
            .await?;

        let reader = BufReader::new(response.into_body());
        let mut stream = reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => Some(parse_binder_build_response(&line)),
                    Err(_error) => None,
                }
            })
            .boxed();

        while let Some(response) = stream.next().await {
            match response {
                Ok(build_response) => {
                    // Process or render the BinderBuildResponse
                    match build_response.phase {
                        mybinder::Phase::Ready { url, token, .. } => {
                            println!("Binder is ready! URL: {}, Token: {}", url, token);
                            // Here you can connect to the kernel, execute code, etc.
                            break;
                        }
                        mybinder::Phase::Failed { message } => {
                            println!("Binder failed: {:?}", message);
                            break;
                        }
                        _ => {
                            println!("Current phase: {:?}", build_response.phase);
                        }
                    }
                }
                Err(e) => {
                    println!("Error processing response: {:?}", e);
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
        anyhow::Ok(())
    })
    .detach();
}

fn main() {
    App::new().run(app_main);
}
