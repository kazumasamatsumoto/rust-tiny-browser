// ステップ 1: URL を部品に分ける
//
// このファイル 1 つで動きます:
//   実行:      rustc --edition 2021 step01_url.rs && ./step01_url
//   テスト:    rustc --edition 2021 --test step01_url.rs -o step01_url_test && ./step01_url_test
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
}

fn main() {
    let inputs = [
        "http://example.com",
        "http://localhost:8000/docs/index.html?q=rust&page=2#top",
        "HTTP://Example.COM:8080",
        "https://example.com",
        "example.com",
        "http://example.com:80a/",
    ];
    for input in inputs {
        println!("{input}");
        match url::parse(input) {
            Ok(u) => {
                println!("  scheme = {}", u.scheme);
                println!("  host   = {}", u.host);
                println!("  port   = {}", u.port);
                println!("  path   = {}", u.path);
                println!("  query  = {:?}", u.query);
                println!("  → request-target: {}   Host ヘッダー: {}", u.request_target(), u.host_header());
            }
            Err(e) => println!("  → エラー: {e}"),
        }
        println!();
    }
}
