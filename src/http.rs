//! HTTP/1.1 のクライアント。TCP でつないで GET を 1 回送り、レスポンスを分解する。
//!
//! 仕様は RFC 9112(HTTP/1.1 のメッセージ構文)と RFC 9110(意味論)。
//!
//! ```text
//! HTTP-message = start-line CRLF *( field-line CRLF ) CRLF [ message-body ]
//! request-line = method SP request-target SP HTTP-version
//! status-line  = HTTP-version SP status-code SP [ reason-phrase ]
//! field-line   = field-name ":" OWS field-value OWS
//! ```
//!
//! この実装の割り切り:
//! - 送るのは GET だけ。`Connection: close` を付けて、サーバーが閉じるまで読み切る
//! - 本文の長さは Content-Length と chunked だけを見る(RFC 9112 §6.3 の順番のうち使うものだけ)
//! - 本文は UTF-8 として読む(文字化けしても落ちないように lossy 変換)
//! - リダイレクト・圧縮・Keep-Alive・HTTPS は扱わない

use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::url::Url;

/// ヘッダー 1 行(field-line)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// 分解したレスポンス。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub version: String,
    pub status_code: u16,
    pub reason_phrase: String,
    pub headers: Vec<Header>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// 名前解決・接続・送受信の失敗(中身は OS からのメッセージ)
    Network(String),
    /// 1 バイトも返ってこなかった
    EmptyResponse,
    /// status-line が "HTTP/x.y 3桁 理由" の形でない
    InvalidStatusLine(String),
    /// field-line に ":" が無い
    InvalidHeader(String),
    /// chunked 以外の Transfer-Encoding、または壊れた chunk
    UnsupportedBody(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Network(m) => write!(f, "通信に失敗: {m}"),
            HttpError::EmptyResponse => write!(f, "レスポンスが空です"),
            HttpError::InvalidStatusLine(l) => write!(f, "ステータス行が不正: {l:?}"),
            HttpError::InvalidHeader(l) => write!(f, "ヘッダーが不正: {l:?}"),
            HttpError::UnsupportedBody(m) => write!(f, "本文を読めません: {m}"),
        }
    }
}

impl std::error::Error for HttpError {}

/// 送信するリクエストの文字列を組み立てる。
///
/// 行末は CRLF。Host は HTTP/1.1 で必須(RFC 9112 §3.2)。
/// `Connection: close` は「この 1 往復で接続を閉じる」宣言で、
/// これがあると「サーバーが閉じるまで読む」だけで本文の終わりが分かる。
pub fn build_request(url: &Url) -> String {
    format!(
        "GET {target} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: tinybrowser/0.1\r\n\
         Accept: text/html\r\n\
         Connection: close\r\n\
         \r\n",
        target = url.request_target(),
        host = url.host_header(),
    )
}

/// URL に GET を送り、レスポンスを返す。
pub fn fetch(url: &Url) -> Result<HttpResponse, HttpError> {
    // 1. 名前解決: "example.com:80" → IP アドレス(OS のリゾルバに任せる)
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| HttpError::Network(format!("名前解決に失敗 {}: {e}", url.host)))?
        .next()
        .ok_or_else(|| HttpError::Network(format!("名前解決の結果が空: {}", url.host)))?;

    // 2. TCP 接続(3 ウェイハンドシェイクはここで起きる)
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| HttpError::Network(format!("接続に失敗 {addr}: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| HttpError::Network(e.to_string()))?;

    // 3. リクエストを送る
    stream
        .write_all(build_request(url).as_bytes())
        .map_err(|e| HttpError::Network(format!("送信に失敗: {e}")))?;

    // 4. サーバーが閉じるまで読む(Connection: close を送ったので、閉じたら終わり)
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| HttpError::Network(format!("受信に失敗: {e}")))?;

    // 5. 分解する
    HttpResponse::parse(&String::from_utf8_lossy(&raw))
}

impl HttpResponse {
    /// 生のレスポンス文字列を分解する。
    ///
    /// 行末は CRLF が正式だが、RFC 9112 §2.2 は「受信側は LF 単独も行末と認識してよい」と
    /// しているので、ここでも LF だけの行を受け付ける。
    pub fn parse(raw: &str) -> Result<HttpResponse, HttpError> {
        if raw.is_empty() {
            return Err(HttpError::EmptyResponse);
        }

        // ヘッダー部と本文の境目 = 最初の空行(CRLF CRLF、または LF LF)
        let (head, body) = split_head_body(raw);

        let mut lines = head.lines(); // lines() は "\r\n" も "\n" も行末として扱う
        let status_line = lines.next().unwrap_or("");

        // status-line = HTTP-version SP status-code SP [ reason-phrase ]
        let mut parts = status_line.splitn(3, ' ');
        let version = parts.next().unwrap_or("");
        let code = parts.next().unwrap_or("");
        let reason_phrase = parts.next().unwrap_or("").trim_end().to_string();
        if !version.starts_with("HTTP/") || code.len() != 3 {
            return Err(HttpError::InvalidStatusLine(status_line.to_string()));
        }
        let status_code = code
            .parse::<u16>()
            .map_err(|_| HttpError::InvalidStatusLine(status_line.to_string()))?;

        // field-line = field-name ":" OWS field-value OWS
        let mut headers = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| HttpError::InvalidHeader(line.to_string()))?;
            headers.push(Header {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            });
        }

        let mut response = HttpResponse {
            version: version.to_string(),
            status_code,
            reason_phrase,
            headers,
            body: String::new(),
        };

        // 本文の長さの決め方(RFC 9112 §6.3 の順番): Transfer-Encoding が Content-Length より優先
        response.body = match response.header("Transfer-Encoding") {
            Some(te) if te.eq_ignore_ascii_case("chunked") => decode_chunked(body)?,
            Some(te) => return Err(HttpError::UnsupportedBody(format!("Transfer-Encoding: {te}"))),
            None => match response.header("Content-Length").and_then(|v| v.trim().parse::<usize>().ok()) {
                // Content-Length があればそのバイト数まで(文字の途中で切れるなら全部)
                Some(len) => body.get(..len).unwrap_or(body).to_string(),
                // どちらも無ければ「接続が閉じるまで」= 受け取った全部
                None => body.to_string(),
            },
        };
        Ok(response)
    }

    /// ヘッダーを名前で引く。field-name は大文字小文字を区別しない(RFC 9110 §5.1)
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

/// 最初の空行でヘッダー部と本文に分ける。CRLF CRLF と LF LF の早い方を採る。
fn split_head_body(raw: &str) -> (&str, &str) {
    let crlf = raw.find("\r\n\r\n").map(|i| (i, 4));
    let lf = raw.find("\n\n").map(|i| (i, 2));
    match (crlf, lf) {
        (Some((a, la)), Some((b, lb))) => {
            if a <= b {
                (&raw[..a], &raw[a + la..])
            } else {
                (&raw[..b], &raw[b + lb..])
            }
        }
        (Some((a, la)), None) => (&raw[..a], &raw[a + la..]),
        (None, Some((b, lb))) => (&raw[..b], &raw[b + lb..]),
        (None, None) => (raw, ""),
    }
}

/// chunked 転送符号化を解く。
///
/// ```text
/// chunked-body = *chunk last-chunk trailer-section CRLF
/// chunk        = chunk-size [ chunk-ext ] CRLF chunk-data CRLF
/// last-chunk   = 1*("0") [ chunk-ext ] CRLF
/// ```
fn decode_chunked(body: &str) -> Result<String, HttpError> {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let (size_line, after) = rest
            .split_once('\n')
            .ok_or_else(|| HttpError::UnsupportedBody("chunk-size の行がありません".into()))?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| HttpError::UnsupportedBody(format!("chunk-size が 16 進でない: {size_hex:?}")))?;
        if size == 0 {
            break; // last-chunk。trailer は読まない
        }
        let data = after
            .get(..size)
            .ok_or_else(|| HttpError::UnsupportedBody("chunk-data が足りません".into()))?;
        out.push_str(data);
        // chunk-data の後ろの CRLF(または LF)を飛ばす
        rest = &after[size..];
        rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')).unwrap_or(rest);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url;

    #[test]
    fn request_line_and_host() {
        let u = url::parse("http://example.com/index.html?a=1").unwrap();
        let req = build_request(&u);
        assert!(req.starts_with("GET /index.html?a=1 HTTP/1.1\r\n"));
        assert!(req.contains("Host: example.com\r\n"));
        assert!(req.ends_with("\r\n\r\n"));

        let u = url::parse("http://localhost:8000/").unwrap();
        assert!(build_request(&u).contains("Host: localhost:8000\r\n"));
    }

    #[test]
    fn parse_crlf_response() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 12\r\n\r\n<p>hello</p>";
        let r = HttpResponse::parse(raw).unwrap();
        assert_eq!(r.version, "HTTP/1.1");
        assert_eq!(r.status_code, 200);
        assert_eq!(r.reason_phrase, "OK");
        assert_eq!(r.header("content-type"), Some("text/html")); // 名前は大文字小文字を区別しない
        assert_eq!(r.body, "<p>hello</p>");
    }

    #[test]
    fn parse_lf_only_response() {
        let raw = "HTTP/1.1 404 Not Found\nContent-Length: 9\n\nnot found";
        let r = HttpResponse::parse(raw).unwrap();
        assert_eq!(r.status_code, 404);
        assert_eq!(r.reason_phrase, "Not Found");
        assert_eq!(r.body, "not found");
    }

    #[test]
    fn content_length_cuts_the_body() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello world";
        assert_eq!(HttpResponse::parse(raw).unwrap().body, "hello");
    }

    #[test]
    fn chunked_body() {
        let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n7\r\n, world\r\n0\r\n\r\n";
        assert_eq!(HttpResponse::parse(raw).unwrap().body, "hello, world");
    }

    #[test]
    fn errors() {
        assert_eq!(HttpResponse::parse(""), Err(HttpError::EmptyResponse));
        assert!(matches!(HttpResponse::parse("<html></html>"), Err(HttpError::InvalidStatusLine(_))));
        assert!(matches!(HttpResponse::parse("HTTP/1.1 200 OK\r\nbroken header\r\n\r\n"), Err(HttpError::InvalidHeader(_))));
        assert!(matches!(
            HttpResponse::parse("HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nx"),
            Err(HttpError::UnsupportedBody(_))
        ));
    }
}
