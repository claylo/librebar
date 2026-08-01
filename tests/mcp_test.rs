#![allow(missing_docs)]
#![cfg(feature = "mcp")]

#[test]
fn transport_stdio_type_is_accessible() {
    fn assert_server_transport<T>(_transport: T)
    where
        T: librebar::mcp::rmcp::transport::IntoTransport<
                librebar::mcp::rmcp::RoleServer,
                std::io::Error,
                librebar::mcp::rmcp::transport::async_rw::TransportAdapterAsyncRW,
            >,
    {
    }

    assert_server_transport(librebar::mcp::transport_stdio());
}

#[test]
fn service_ext_trait_is_accessible() {
    #[allow(unused_imports)]
    use librebar::mcp::ServiceExt as _;
}
