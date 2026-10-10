//! Routes and listener for the deck.

use std::io;
use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use maud::Markup;
use tokio::net::TcpListener;

use crate::{views, DeckError, DeckState, SessionRow};

/// Build the router. `GET /` renders the page and `GET /settle` renders the queue fragment.
pub fn router(state: DeckState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/settle", get(settle))
        .with_state(state)
}

/// Bind the listener. Returns `ServerBindFailed` when the address is taken or denied.
pub async fn bind(addr: SocketAddr) -> Result<TcpListener, DeckError> {
    TcpListener::bind(addr)
        .await
        .map_err(|source| DeckError::ServerBindFailed { addr, source })
}

/// Serve the router on a bound listener until the listener fails.
pub async fn serve(listener: TcpListener, state: DeckState) -> io::Result<()> {
    axum::serve(listener, router(state)).await
}

async fn root(State(state): State<DeckState>) -> Markup {
    let rows = session_rows(&state);
    views::page(&rows, settle_fragment(&state))
}

async fn settle(State(state): State<DeckState>) -> Markup {
    settle_fragment(&state)
}

fn session_rows(state: &DeckState) -> Vec<SessionRow> {
    state
        .sessions()
        .session_logs()
        .iter()
        .filter_map(|log| SessionRow::from_events(log))
        .collect()
}

/// Fail closed: a settle source error renders the notice with HTTP 200, never a card.
fn settle_fragment(state: &DeckState) -> Markup {
    match state.settle().pending_cards() {
        Ok(cards) => views::settle_cards(&cards),
        Err(_) => views::settle_disconnected(),
    }
}
