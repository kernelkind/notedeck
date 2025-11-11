use std::{error::Error, fmt};

use http_body_util::{BodyExt, Empty};
use hyper::{
    body::{Buf, Bytes},
    header::{self},
    Request, Uri,
};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use url::Url;

pub async fn http_req(url: &str) -> Result<HyperHttpResponse, HyperHttpError> {
    let mut current_uri: Uri = url.parse().map_err(|_| HyperHttpError::Uri)?;

    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| HyperHttpError::Hyper(Box::new(e)))?
        .https_or_http()
        .enable_http1()
        .build();

    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build(https);

    const MAX_REDIRECTS: usize = 5;
    let mut redirects = 0;

    let res = loop {
        let authority = current_uri
            .authority()
            .ok_or(HyperHttpError::Host)?
            .clone();

        // Fetch the url...
        let req = Request::builder()
            .uri(current_uri.clone())
            .header(hyper::header::HOST, authority.as_str())
            .body(Empty::<Bytes>::new())
            .map_err(|e| HyperHttpError::Hyper(Box::new(e)))?;

        let res = client
            .request(req)
            .await
            .map_err(|e| HyperHttpError::Hyper(Box::new(e)))?;

        if res.status().is_redirection() {
            if redirects >= MAX_REDIRECTS {
                return Err(HyperHttpError::TooManyRedirects);
            }

            let location_header = res
                .headers()
                .get(header::LOCATION)
                .ok_or(HyperHttpError::MissingRedirectLocation)?
                .clone();

            let location = location_header
                .to_str()
                .map_err(|_| HyperHttpError::InvalidRedirectLocation)?
                .to_string();

            res.into_body()
                .collect()
                .await
                .map_err(|e| HyperHttpError::Hyper(Box::new(e)))?;

            current_uri = resolve_redirect(&current_uri, &location)?;
            redirects += 1;
            continue;
        } else {
            break res;
        }
    };

    let content_type = res
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|t| t.to_str().ok())
        .map(|s| s.to_string());

    let len = res
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or(HyperHttpError::NoContentLength)?;

    let mut body = res
        .collect()
        .await
        .map_err(|e| HyperHttpError::Hyper(Box::new(e)))?
        .aggregate();

    let bytes = body.copy_to_bytes(len);

    Ok(HyperHttpResponse {
        content_type,
        bytes: bytes.into(),
    })
}

#[derive(Debug)]
pub enum HyperHttpError {
    Hyper(Box<dyn std::error::Error + Send + Sync>),
    Host,
    Uri,
    NoContentLength,
    TooManyRedirects,
    MissingRedirectLocation,
    InvalidRedirectLocation,
}

#[derive(Debug)]
pub struct HyperHttpResponse {
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

impl Error for HyperHttpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Hyper(e) => Some(&**e),
            _ => None,
        }
    }
}

impl fmt::Display for HyperHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hyper(e) => write!(f, "Hyper error: {}", e),
            Self::Host => write!(f, "Missing host in URL"),
            Self::Uri => write!(f, "Invalid URI"),
            Self::NoContentLength => write!(f, "Missing Content-Length header"),
            Self::TooManyRedirects => write!(f, "Too many redirect responses"),
            Self::MissingRedirectLocation => write!(f, "Redirect response missing Location header"),
            Self::InvalidRedirectLocation => write!(f, "Invalid redirect Location header"),
        }
    }
}

fn resolve_redirect(current: &Uri, location: &str) -> Result<Uri, HyperHttpError> {
    if let Ok(uri) = location.parse::<Uri>() {
        if uri.scheme().is_some() {
            return Ok(uri);
        }
    }

    let base = Url::parse(&current.to_string()).map_err(|_| HyperHttpError::Uri)?;
    let joined = base
        .join(location)
        .map_err(|_| HyperHttpError::InvalidRedirectLocation)?;

    joined
        .as_str()
        .parse::<Uri>()
        .map_err(|_| HyperHttpError::InvalidRedirectLocation)
}

#[tokio::test]
async fn http_req_rejects_https_scheme() {
    crate::app::install_crypto();
    let res = http_req("https://nostr.build/i/nostr.build_95bb7ab1602652b152795511012747fafcbc040bf0adac220cd833cc5a0ff817.jpeg").await.unwrap();
    println!("{:?}", res.content_type);
}
