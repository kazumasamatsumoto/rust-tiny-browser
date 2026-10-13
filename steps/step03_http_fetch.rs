// ステップ 3: TCP でつないで GET を送り、本物のサーバーから受け取る(ネットワークが要る)
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step03_http_fetch.rs && ./step03_http_fetch
//   テスト:    rustc --edition 2021 --test step03_http_fetch.rs -o step03_http_fetch_test && ./step03_http_fetch_test
// cargo のプロジェクトでは、各 `mod` の中身がそのまま src/ 以下のファイルになります。
#![allow(dead_code)] // 部品を全部 1 ファイルに入れているので、このステップで使わない関数もある

mod url {
    //! URL を部品に分ける。
    //!
    //! 仕様は WHATWG URL Standard(https://url.spec.whatwg.org/)。
    //! URL は scheme / host / port / path / query / fragment からなり、http は「特別なスキーム」で既定ポートは 80。
    //!
    //! この実装の割り切り:
    //! - 受け付けるスキームは http だけ(TLS を実装しないので https は扱えない)
    //! - 相対 URL、パーセント符号化、IPv6 のホスト表記、ユーザー情報(user:pass@)は扱わない
    //! - fragment(#以降)はサーバーに送らないので読み捨てる

    use std::fmt;

    /// http の既定ポート(URL Standard の special scheme の表より)
    pub const HTTP_DEFAULT_PORT: u16 = 80;

    /// 分解した URL。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Url {
        pub scheme: String,
        pub host: String,
        pub port: u16,
        /// 先頭の "/" を含む。URL にパスが無ければ "/"
        pub path: String,
        /// "?" より後ろ。無ければ None
        pub query: Option<String>,
    }

    /// 分解に失敗した理由。
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum UrlError {
        /// "://" が無い
        MissingScheme,
        /// http 以外のスキーム
        UnsupportedScheme(String),
        /// ホストが空(例: http:///path)
        EmptyHost,
        /// ポートが 0〜65535 の整数でない
        InvalidPort(String),
    }

    impl fmt::Display for UrlError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UrlError::MissingScheme => write!(f, "スキームがありません(例: http://...)"),
                UrlError::UnsupportedScheme(s) => write!(f, "対応していないスキームです: {s}(http だけ)"),
                UrlError::EmptyHost => write!(f, "ホストが空です"),
                UrlError::InvalidPort(p) => write!(f, "ポートが不正です: {p:?}"),
            }
        }
    }

    impl std::error::Error for UrlError {}

    /// 文字列を URL に分解する。
    ///
    /// 手順は左から右へ 4 段階:
    /// 1. "://" の前がスキーム
    /// 2. 次の "/" か "?" までが authority(host[:port])
    /// 3. authority を ":" で host と port に分ける(無ければ 80)
    /// 4. 残りを "?" で path と query に分ける
    pub fn parse(input: &str) -> Result<Url, UrlError> {
        let input = input.trim();

        // 1. スキーム
        let (scheme, rest) = input.split_once("://").ok_or(UrlError::MissingScheme)?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" {
            return Err(UrlError::UnsupportedScheme(scheme));
        }

        // fragment はクライアント内でだけ使うもの。サーバーには送らないのでここで捨てる
        let rest = rest.split('#').next().unwrap_or("");

        // 2. authority の終わり = 最初の "/" か "?"
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let tail = &rest[authority_end..];

        // 3. host と port
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if p.is_empty() => (h, HTTP_DEFAULT_PORT), // "host:" は既定ポート扱い
            Some((h, p)) => {
                let port = p
                    .parse::<u16>()
                    .map_err(|_| UrlError::InvalidPort(p.to_string()))?;
                (h, port)
            }
            None => (authority, HTTP_DEFAULT_PORT),
        };
        if host.is_empty() {
            return Err(UrlError::EmptyHost);
        }

        // 4. path と query
        let (path, query) = match tail.split_once('?') {
            Some((p, q)) => (p, Some(q.to_string())),
            None => (tail, None),
        };
        let path = if path.is_empty() { "/".to_string() } else { path.to_string() };

        Ok(Url {
            scheme,
            host: host.to_ascii_lowercase(),
            port,
            path,
            query,
        })
    }

    impl Url {
        /// HTTP の request-target(origin-form)。path と ?query をつないだもの
        pub fn request_target(&self) -> String {
            match &self.query {
                Some(q) => format!("{}?{}", self.path, q),
                None => self.path.clone(),
            }
        }

        /// Host ヘッダーの値。既定ポート以外のときだけ ":port" を付ける(RFC 9110 §7.2)
        pub fn host_header(&self) -> String {
            if self.port == HTTP_DEFAULT_PORT {
                self.host.clone()
            } else {
                format!("{}:{}", self.host, self.port)
            }
        }
    }

    impl fmt::Display for Url {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}://{}{}", self.scheme, self.host_header(), self.request_target())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn host_only() {
            let u = parse("http://example.com").unwrap();
            assert_eq!(u.host, "example.com");
            assert_eq!(u.port, 80);
            assert_eq!(u.path, "/");
            assert_eq!(u.query, None);
        }

        #[test]
        fn port_path_query() {
            let u = parse("http://localhost:8000/docs/index.html?q=rust&page=2").unwrap();
            assert_eq!(u.port, 8000);
            assert_eq!(u.path, "/docs/index.html");
            assert_eq!(u.query.as_deref(), Some("q=rust&page=2"));
            assert_eq!(u.request_target(), "/docs/index.html?q=rust&page=2");
            assert_eq!(u.host_header(), "localhost:8000");
        }

        #[test]
        fn query_without_path() {
            let u = parse("http://example.com?x=1").unwrap();
            assert_eq!(u.path, "/");
            assert_eq!(u.query.as_deref(), Some("x=1"));
        }

        #[test]
        fn fragment_is_dropped() {
            let u = parse("http://example.com/a#section-2").unwrap();
            assert_eq!(u.path, "/a");
            assert_eq!(u.query, None);
        }

        #[test]
        fn scheme_and_host_are_lowercased() {
            let u = parse("HTTP://Example.COM/Path").unwrap();
            assert_eq!(u.scheme, "http");
            assert_eq!(u.host, "example.com");
            assert_eq!(u.path, "/Path"); // パスは大文字小文字を区別する
        }

        #[test]
        fn errors() {
            assert_eq!(parse("example.com"), Err(UrlError::MissingScheme));
            assert_eq!(parse("https://example.com"), Err(UrlError::UnsupportedScheme("https".into())));
            assert_eq!(parse("http:///path"), Err(UrlError::EmptyHost));
            assert_eq!(parse("http://example.com:80a/"), Err(UrlError::InvalidPort("80a".into())));
            assert_eq!(parse("http://example.com:70000/"), Err(UrlError::InvalidPort("70000".into())));
        }
    }
}

mod http {
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
}

fn main() {
    // 使い方: ./step03_http_fetch http://localhost:8000/index.html
    // ローカルサーバーは  python3 -m http.server 8000  で立つ(index.html を置いたフォルダで)
    let target = std::env::args().nth(1).unwrap_or_else(|| "http://example.com/".to_string());
    let u = match url::parse(&target) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("URL が不正: {e}");
            std::process::exit(1);
        }
    };
    println!("→ {} に接続して GET {} を送ります", u.host_header(), u.request_target());
    match http::fetch(&u) {
        Ok(r) => {
            println!("{} {} {}", r.version, r.status_code, r.reason_phrase);
            for h in &r.headers {
                println!("  {}: {}", h.name, h.value);
            }
            println!("--- body({} 文字。先頭 400 文字) ---", r.body.chars().count());
            println!("{}", r.body.chars().take(400).collect::<String>());
        }
        Err(e) => eprintln!("失敗: {e}"),
    }
}
