use std::error::Error;
use std::fmt;
use std::str::{FromStr, Utf8Error};

use hyper::body::Bytes;
use hyper::client::HttpConnector;
use hyper::http::uri::InvalidUri;
use hyper::{Body, Client, Response, StatusCode, Uri};
use ouroboros::self_referencing;
use roxmltree_serde::{from_doc, Document, ParseError as XmlParseError};

use crate::feed::Feed;

#[cfg(feature = "rustls")]
type Connector = hyper_rustls::HttpsConnector<HttpConnector>;
#[cfg(not(feature = "rustls"))]
type Connector = HttpConnector;

#[derive(Debug)]
pub struct RssClient {
    client: Client<Connector, Body>,
}

impl RssClient {
    const MAX_REDIRECT: usize = 10;

    pub async fn exec(&self, mut req: RssRequest) -> Result<RssResponse<Bytes>, RssError> {
        let mut redir_count = 0;
        loop {
            let origin = req.uri.clone();
            let resp = self
                .client
                .get(req.uri)
                .await
                .map_err(RssError::HttpStreamError)?;

            if resp.status().is_success() {
                let body = hyper::body::to_bytes(resp.into_body())
                    .await
                    .map_err(RssError::HttpStreamError)?;
                return RssResponse::from_bytes(body);
            } else if resp.status().is_redirection() {
                if redir_count > Self::MAX_REDIRECT {
                    return Err(RssError::TooManyRedirects);
                } else if let Some(redir_uri) = Self::redirect_uri(&origin, &resp) {
                    req = RssRequest { uri: redir_uri };
                    redir_count += 1;
                } else {
                    return Err(RssError::UnexpectedResponse(resp.status()));
                }
            } else {
                return Err(RssError::UnexpectedResponse(resp.status()));
            }
        }
    }

    fn redirect_uri<A>(origin: &Uri, resp: &Response<A>) -> Option<Uri> {
        let loc = resp.headers().get("Location")?;
        let mut uri_parts = Uri::from_str(loc.to_str().ok()?).ok()?.into_parts();
        if let (None, Some(scheme)) = (&uri_parts.scheme, origin.scheme()) {
            uri_parts.scheme = Some(scheme.clone());
        }
        if let (None, Some(authority)) = (&uri_parts.authority, origin.authority()) {
            uri_parts.authority = Some(authority.clone());
        }
        uri_parts.try_into().ok()
    }
}

impl Default for RssClient {
    fn default() -> Self {
        #[cfg(feature = "rustls")]
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        #[cfg(not(feature = "rustls"))]
        let connector = HttpConnector::new();

        let client = Client::builder().build(connector);
        Self { client }
    }
}

#[derive(Debug)]
pub struct RssRequest {
    uri: Uri,
}

impl RssRequest {
    pub fn new(str: &str) -> Result<Self, RssError> {
        let uri = str
            .parse()
            .map_err(|e: InvalidUri| RssError::HttpError(e.into()))?;
        Ok(Self { uri })
    }
}

#[self_referencing]
pub struct RssResponse<A: 'static> {
    body: A,
    #[borrows(body)]
    #[covariant]
    document: Document<'this>,
    #[borrows(document)]
    #[covariant]
    pub feed: Feed<'this>,
}

impl<A: AsRef<[u8]>> RssResponse<A> {
    pub fn from_bytes(body: A) -> Result<Self, RssError> {
        RssResponseTryBuilder {
            body,
            document_builder: |body| {
                let str = std::str::from_utf8(body.as_ref()).map_err(RssError::EncodingError)?;
                Document::parse(str).map_err(RssError::XmlParse)
            },
            feed_builder: |doc| from_doc(doc).map_err(RssError::XmlDecode),
        }
        .try_build()
    }
}

#[derive(Debug)]
pub enum RssError {
    HttpError(hyper::http::Error),
    HttpStreamError(hyper::Error),
    EncodingError(Utf8Error),
    UnexpectedResponse(StatusCode),
    XmlDecode(roxmltree_serde::Error),
    XmlParse(XmlParseError),
    TooManyRedirects,
}

impl fmt::Display for RssError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RssError::HttpError(err) => write!(f, "http error: {err}"),
            RssError::HttpStreamError(err) => write!(f, "http error: {err}"),
            RssError::EncodingError(err) => write!(f, "encoding error: {err}"),
            RssError::UnexpectedResponse(code) => write!(f, "unexpected response code {code}"),
            RssError::XmlDecode(err) => write!(f, "xml decode error: {err}"),
            RssError::XmlParse(err) => write!(f, "xml parse error: {err}"),
            RssError::TooManyRedirects => write!(f, "too many redirects"),
        }
    }
}

impl Error for RssError {}
