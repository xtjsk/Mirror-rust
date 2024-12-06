use crate::mirror::authenticators::basic_authenticator::BasicAuthenticator;
use crate::mirror::authenticators::network_authenticator::NetworkAuthenticatorTrait;
use crate::mirror::core::network_manager::{
    NetworkManager, NetworkManagerStatic, NetworkManagerTrait,
};
use crate::mirror::core::network_server::{NetworkServer, NetworkServerStatic};
use crate::mirror::core::network_start_position::NetworkStartPosition;
use crate::mirror::core::network_time::NetworkTime;
use crate::mirror::core::transport::TransportTrait;
use crate::mirror::transports::kcp2k::kcp2k_transport::Kcp2kTransport;
use crate::{log_debug, log_warn};
use signal_hook::iterator::Signals;
use std::thread;
use std::time::Duration;

pub fn stop_signal() -> &'static mut bool {
    static mut STOP: bool = false;
    unsafe { &mut STOP }
}

pub struct NetworkLoop;

impl NetworkLoop {
    // 1
    fn awake() {
        Kcp2kTransport::awake();
        NetworkStartPosition::awake();
        NetworkManager::awake();
    }

    // 2
    fn on_enable() {
        BasicAuthenticator::new("123".to_string(), "456".to_string()).enable();
    }

    // 3
    fn start() {
        let network_manager_singleton = NetworkManagerStatic::get_network_manager_singleton();
        network_manager_singleton.start();

        NetworkServerStatic::for_each_network_message_handler(|item| {
            log_debug!(format!(
                "message hash: {} require_authentication: {}",
                item.key(),
                item.require_authentication
            ));
        });

        NetworkServerStatic::for_each_network_connection(|item| {
            log_debug!(format!(
                "connection hash: {} address: {}",
                item.key(),
                item.address
            ));
        });
    }

    // 4
    fn fixed_update() {
        // NetworkBehaviour fixed_update  模拟
        NetworkServerStatic::spawned_network_identities()
            .iter_mut()
            .for_each(|mut identity| {
                identity
                    .network_behaviours
                    .iter_mut()
                    .for_each(|behaviour| {
                        behaviour.fixed_update();
                    });
            });
    }

    // 5
    fn update() {
        // NetworkEarlyUpdate
        // AddToPlayerLoop(NetworkEarlyUpdate, typeof(NetworkLoop), ref playerLoop, typeof(EarlyUpdate), AddMode.End);
        NetworkServer::network_early_update();

        // NetworkManager update
        NetworkManagerStatic::get_network_manager_singleton().update();

        // NetworkBehaviour update  模拟
        NetworkServerStatic::spawned_network_identities()
            .iter_mut()
            .for_each(|mut identity| {
                identity
                    .network_behaviours
                    .iter_mut()
                    .for_each(|behaviour| {
                        behaviour.update();
                    });
            });
    }

    // 6
    fn late_update() {
        // NetworkLateUpdate
        // AddToPlayerLoop(NetworkLateUpdate, typeof(NetworkLoop), ref playerLoop, typeof(PreLateUpdate), AddMode.End);
        NetworkServer::network_late_update();

        // NetworkBehaviour late_update  模拟
        NetworkManagerStatic::get_network_manager_singleton().late_update();

        // NetworkBehaviour late_update
        NetworkServerStatic::spawned_network_identities()
            .iter_mut()
            .for_each(|mut identity| {
                identity
                    .network_behaviours
                    .iter_mut()
                    .for_each(|behaviour| behaviour.late_update());
            });
    }

    // 7
    fn on_disable() {
        NetworkManagerStatic::get_network_manager_singleton().dis_enable_authenticator();
    }

    // 8
    fn on_destroy() {
        NetworkManager::shutdown();
    }

    pub fn run() {
        // 注册信号处理函数
        let mut signals_info =
            Signals::new(&[signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
                .expect("Failed to register signal handler");

        // 启动一个线程来监听终止信号
        thread::spawn(move || {
            for sig in signals_info.forever() {
                println!("\nSignal: {:?}", sig);
                *stop_signal() = true;
                break;
            }
        });


        // 1
        Self::awake();
        // 2
        Self::on_enable();
        // 3
        Self::start();

        // 目标帧率
        let target_frame_time = Duration::from_secs(1) / NetworkServerStatic::tick_rate();
        while !*stop_signal() {
            Self::fixed_update();
            Self::update();
            Self::late_update();
            NetworkTime::increment_frame_count();
            let mut sleep_time = Duration::from_secs(0);
            match NetworkServerStatic::full_update_duration().try_read() {
                Ok(full_update_duration) => {
                    // 计算平均耗费时间
                    let average_elapsed_time = Duration::from_secs_f64(full_update_duration.average());
                    // 如果平均耗费时间小于目标帧率
                    if average_elapsed_time < target_frame_time {
                        // 计算帧平均补偿睡眠时间
                        sleep_time =
                            (target_frame_time - average_elapsed_time) / NetworkTime::frame_count();
                    }
                }
                Err(e) => {
                    log_warn!(format!(
                        "Server.network_late_update() full_update_duration error: {}",
                        e
                    ));
                }
            }
            // 休眠
            thread::sleep(sleep_time);
        }

        Self::on_disable();
        Self::on_destroy();
    }
}
