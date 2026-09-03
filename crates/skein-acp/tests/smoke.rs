//! T1 smoke test: the two structural assumptions the facade's design rests on.
//! Throwaway — it exercises the SDK, not `skein_acp`.

use agent_client_protocol::schema::v1::ContentBlock;
use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, StopReason, TextContent, ToolCallUpdate,
    ToolCallUpdateFields,
};
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, Responder};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn responder_and_permission_round_trip_from_a_plain_thread() {
    let (agent_side, client_side) = tokio::io::duplex(8192);
    let (agent_read, agent_write) = tokio::io::split(agent_side);
    let (client_read, client_write) = tokio::io::split(client_side);

    let agent = tokio::spawn(async move {
        Agent
            .builder()
            .name("smoke-agent")
            .on_receive_request(
                async move |req: PromptRequest,
                            responder: Responder<PromptResponse>,
                            cx: ConnectionTo<Client>| {
                    // Assumption 1: the handler returns immediately and the
                    // Responder is answered later, from an OS thread.
                    std::thread::spawn(move || {
                        // Assumption 2: a request can be issued and awaited from
                        // a non-dispatch thread with no executor at all.
                        let (tx, rx) = std::sync::mpsc::channel();
                        cx.send_request(RequestPermissionRequest::new(
                            req.session_id.clone(),
                            ToolCallUpdate::new("tc-1", ToolCallUpdateFields::new()),
                            vec![PermissionOption::new(
                                "allow-once",
                                "Allow once",
                                PermissionOptionKind::AllowOnce,
                            )],
                        ))
                        .on_receiving_result(move |res| {
                            let _ = tx.send(res.map(|r| r.outcome));
                            async { Ok(()) }
                        })
                        .expect("permission request accepted");
                        let outcome = rx.recv().expect("permission answered");
                        let stop = match outcome {
                            Ok(RequestPermissionOutcome::Selected(_)) => StopReason::EndTurn,
                            _ => StopReason::Refusal,
                        };
                        responder
                            .respond(PromptResponse::new(stop))
                            .expect("responded");
                    });
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                agent_write.compat_write(),
                agent_read.compat(),
            ))
            .await
    });

    let stop = Client
        .builder()
        .name("smoke-client")
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder, _cx| {
                let id = req.options.first().map(|o| o.option_id.clone()).unwrap();
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(
            ByteStreams::new(client_write.compat_write(), client_read.compat()),
            async |cx: ConnectionTo<Agent>| {
                let resp = cx
                    .send_request(PromptRequest::new(
                        SessionId::new("smoke"),
                        vec![ContentBlock::Text(TextContent::new("go"))],
                    ))
                    .block_task()
                    .await?;
                Ok(resp.stop_reason)
            },
        )
        .await
        .expect("client ran");

    assert_eq!(stop, StopReason::EndTurn);
    drop(agent);
}
