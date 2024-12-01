use async_dispatcher::{set_dispatcher, Dispatcher, Runnable};
use gpui::{AppContext, PlatformDispatcher};

fn zed_dispatcher(cx: &mut AppContext) -> impl Dispatcher {
    struct ZedDispatcher {
        dispatcher: Arc<dyn PlatformDispatcher>,
    }

    // PlatformDispatcher is _super_ close to the same interface we put in
    // async-dispatcher, except for the task label in dispatch. Later we should
    // just make that consistent so we have this dispatcher ready to go for
    // other crates in Zed.
    impl Dispatcher for ZedDispatcher {
        fn dispatch(&self, runnable: Runnable) {
            self.dispatcher.dispatch(runnable, None)
        }

        fn dispatch_after(&self, duration: Duration, runnable: Runnable) {
            self.dispatcher.dispatch_after(duration, runnable);
        }
    }

    ZedDispatcher {
        dispatcher: cx.background_executor().dispatcher.clone(),
    }
}

#[cfg(test)]
mod test {
    use runtimelib::{ConnectionInfo, JupyterMessageContent};

    use jupyter_protocol::{ExecuteRequest, JupyterKernelspec, JupyterMessage, ReplyStatus};

    #[gpui::test]
    async fn async_dispatcher_smoke_test(cx: &mut gpui::TestAppContext) -> anyhow::Result<()> {
        set_dispatcher(zed_dispatcher(cx));

        // Set up connection info
        let connection_info = ConnectionInfo {
            transport: jupyter_protocol::connection_info::Transport::TCP,
            ip: "127.0.0.1".to_string(),
            stdin_port: 9000,
            control_port: 9001,
            hb_port: 9002,
            shell_port: 9003,
            iopub_port: 9004,
            signature_scheme: "hmac-sha256".to_string(),
            key: uuid::Uuid::new_v4().to_string(),
            kernel_name: Some("python".to_string()),
        };

        let connection_path = "/tmp/connection_info.json";

        std::fs::write(connection_path, serde_json::to_string(&connection_info)?)?;

        let kernelspec = JupyterKernelspec {
            argv: vec![
                "python".to_string(),
                "-m".to_string(),
                "ipykernel_launcher".to_string(),
                "-f".to_string(),
                "{connection_file}".to_string(),
            ],
            display_name: "Python 3 (ipykernel)".to_string(),
            language: "python".to_string(),
            interrupt_mode: Some("signal".to_string()),
            metadata: None,
            env: None,
        };

        let mut cmd = smol::process::Command::new(&kernelspec.argv[0]);

        for arg in &kernelspec.argv[1..] {
            if arg == "{connection_file}" {
                cmd.arg(connection_path);
            } else {
                cmd.arg(arg);
            }
        }

        dbg!("noice");

        let mut process = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        cx.run_until_parked();

        dbg!("noice");

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut iopub_socket =
            runtimelib::create_client_iopub_connection(&connection_info, "", &session_id).await?;
        dbg!("noice");

        cx.run_until_parked();

        let mut shell_socket =
            runtimelib::create_client_shell_connection(&connection_info, &session_id).await?;
        cx.run_until_parked();

        dbg!("noice");

        // Create a simple execute request
        let execute_request = ExecuteRequest::new("print('🐍 '*3)".to_string());
        let execute_request: JupyterMessage = execute_request.into();

        let iopub_task = cx.spawn(|_cx| async move {
            while let Ok(message) = iopub_socket.read().await {
                // looking for the stream content to know we've got it
                match message.content {
                    JupyterMessageContent::StreamContent(stream) => {
                        assert_eq!(stream.text, "🐍 🐍 🐍 \n");
                        break;
                    }
                    _ => {}
                }
            }
        });

        shell_socket.send(execute_request).await?;

        let reply = shell_socket.read().await?;

        match reply.content {
            JupyterMessageContent::ExecuteReply(reply) => {
                assert_eq!(reply.execution_count, 1.into());
                assert_eq!(reply.status, ReplyStatus::Ok);
            }
            _ => {
                panic!("Unexpected message: {:?}", reply);
            }
        }

        iopub_task.await;

        // Here you would normally send the request to the kernel and handle the response
        // For the sake of this smoke test, we'll just print a success message
        println!("Successfully set up async dispatcher runtime with a Python kernel");

        Ok(())
    }
}
