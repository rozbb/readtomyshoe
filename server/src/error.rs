use axum::response::IntoResponse;

#[derive(Debug)]
pub(crate) struct RtmsError(anyhow::Error);

impl RtmsError {
    pub(crate) fn context<C>(self, ctx: C) -> Self
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        RtmsError(self.0.context(ctx))
    }
}

impl From<anyhow::Error> for RtmsError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for RtmsError {
    fn into_response(self) -> axum::response::Response {
        // Log the error and return it
        let err_str = self.0.to_string();
        tracing::error!("{}", err_str);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_str).into_response()
    }
}
