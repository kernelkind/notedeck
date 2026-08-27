#[test]
fn websocket_error_preserves_native_raw_os_error() {
    let source = std::io::Error::from_raw_os_error(libc::EMFILE);
    let error = enostr::WebSocketError::from(source);

    assert_eq!(error.raw_os_error(), Some(libc::EMFILE));
}
