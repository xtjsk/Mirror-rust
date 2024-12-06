use Mirror_rust::mirror::authenticators::basic_authenticator::BasicAuthenticator;
use Mirror_rust::mirror::authenticators::network_authenticator::NetworkAuthenticatorTrait;
use Mirror_rust::mirror::core::network_loop::NetworkLoop;
use Mirror_rust::mirror::core::network_manager::NetworkManagerStatic;
use Mirror_rust::mirror::core::network_start_position::NetworkStartPosition;
use Mirror_rust::mirror::core::transport::TransportTrait;
use Mirror_rust::mirror::transports::kcp2k::kcp2k_transport::Kcp2kTransport;

fn awake() {
    // 传输层初始化
    Kcp2kTransport::awake();
    NetworkStartPosition::awake();
}

fn on_enable() {
    // 启用基础认证
    BasicAuthenticator::new("123".to_string(), "456".to_string()).enable();
}

fn on_disable() {
    // 禁用基础认证
    NetworkManagerStatic::get_network_manager_singleton().dis_enable_authenticator();
}

fn main() {
    // 添加 awake 函数
    NetworkLoop::add_awake_function(awake);
    // 添加 on_enable 函数
    NetworkLoop::add_on_enable_function(on_enable);
    // 添加 on_disable 函数
    NetworkLoop::add_on_disable_function(on_disable);
    // NetworkLoop
    NetworkLoop::run();
}