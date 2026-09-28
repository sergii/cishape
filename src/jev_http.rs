use crate::decision::{JevSystemOneRequest, JevSystemOneResponse};
use anyhow::{Context, Result};

pub const DEFAULT_JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";

#[derive(Debug, Clone)]
pub struct JevHttpClient {
    endpoint: String,
}

impl Default for JevHttpClient {
    fn default() -> Self {
        Self::new(DEFAULT_JEV_ENDPOINT)
    }
}

impl JevHttpClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn send(
        &self,
        api_key: &str,
        request: &JevSystemOneRequest,
    ) -> Result<JevSystemOneResponse> {
        anyhow::ensure!(!api_key.trim().is_empty(), "Jev API key is empty");

        let authorization = format!("Bearer {api_key}");
        let mut response = ureq::post(&self.endpoint)
            .header("Authorization", &authorization)
            .header("Content-Type", "application/json")
            .send_json(request)
            .with_context(|| format!("POST {}", self.endpoint))?;

        response
            .body_mut()
            .read_json::<JevSystemOneResponse>()
            .context("parse Jev System One response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{JevChoiceQuestion, JevSystemOneRequest};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn request() -> JevSystemOneRequest {
        JevSystemOneRequest {
            model: "jev-latest".into(),
            state: json!({"job": "test"}),
            questions: BTreeMap::from([(
                "runner_choice".into(),
                JevChoiceQuestion {
                    question_type: "choice".into(),
                    instructions: "Choose one.".into(),
                    criteria: BTreeMap::from([
                        ("cpu2-mem4".into(), json!({"cost": 1})),
                        ("cpu4-mem8".into(), json!({"cost": 2})),
                    ]),
                },
            )]),
        }
    }

    fn spawn_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("timeout");

            let mut data = Vec::new();
            let mut buffer = [0_u8; 4096];
            let mut expected_len = None;

            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        data.extend_from_slice(&buffer[..count]);

                        if expected_len.is_none()
                            && let Some(header_end) =
                                data.windows(4).position(|window| window == b"\r\n\r\n")
                        {
                            let header_text =
                                String::from_utf8_lossy(&data[..header_end]).into_owned();
                            let content_length = header_text
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0);
                            expected_len = Some(header_end + 4 + content_length);
                        }

                        if expected_len.is_some_and(|length| data.len() >= length) {
                            break;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(error) => panic!("read request: {error}"),
                }
            }

            tx.send(String::from_utf8_lossy(&data).into_owned())
                .expect("send capture");

            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write response");
        });

        (format!("http://{address}/v1/systemone"), rx)
    }

    #[test]
    fn sends_bearer_auth_and_typed_json() {
        let body = r#"{
          "model":"jev-1.13.0",
          "answers":{
            "runner_choice":{
              "type":"choice",
              "choice":"cpu2-mem4",
              "confidence":0.9,
              "probabilities":{"cpu2-mem4":0.9,"cpu4-mem8":0.1}
            }
          },
          "usage":{"input_tokens":10,"output_tokens":2}
        }"#;
        let (endpoint, captured) = spawn_server("HTTP/1.1 200 OK", body);
        let client = JevHttpClient::new(endpoint);

        let response = client.send("secret-key", &request()).expect("send");
        assert_eq!(response.model, "jev-1.13.0");

        let raw = captured.recv_timeout(Duration::from_secs(2)).expect("capture");
        assert!(raw.contains("Authorization: Bearer secret-key"));
        assert!(raw.contains("\"model\":\"jev-latest\""));
        assert!(raw.contains("\"runner_choice\""));
        assert!(raw.contains("\"type\":\"choice\""));
    }

    #[test]
    fn non_success_status_fails_closed() {
        let (endpoint, _captured) =
            spawn_server("HTTP/1.1 401 Unauthorized", r#"{"detail":"unauthorized"}"#);
        let client = JevHttpClient::new(endpoint);

        let error = client.send("bad-key", &request()).expect_err("must fail");
        assert!(error.to_string().contains("POST"));
    }
}
