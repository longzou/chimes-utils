use std::ffi::OsString;
use std::{
    thread::sleep,
    time::{Duration, Instant},
};
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};

pub fn install_windows_service(
    service_name: &str,
    service_disp: &str,
    service_desc: &str,
) -> windows_service::Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    // This example installs the service defined in `examples/ping_service.rs`.
    // In the real world code you would set the executable path to point to your own binary
    // that implements windows service.
    let service_binary_path = ::std::env::current_exe().unwrap();
    let args = OsString::from("-start");

    let service_info = ServiceInfo {
        name: OsString::from(service_name),
        display_name: OsString::from(service_disp),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![args],
        dependencies: vec![],
        account_name: None, // run as System
        account_password: None,
    };
    let service = service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;
    service.set_description(service_desc)?;
    Ok(())
}

pub fn uninstall_windows_service(service_name: &str) -> windows_service::Result<()> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = service_manager.open_service(service_name, service_access)?;

    // The service will be marked for deletion as long as this function call succeeds.
    // However, it will not be deleted from the database until it is stopped and all open handles to it are closed.
    service.delete()?;
    // Our handle to it is not closed yet. So we can still query it.
    if service.query_status()?.current_state != ServiceState::Stopped {
        // If the service cannot be stopped, it will be deleted when the system restarts.
        service.stop()?;
    }
    // Explicitly close our open handle to the service. This is automatically called when `service` goes out of scope.
    drop(service);

    // Win32 API does not give us a way to wait for service deletion.
    // To check if the service is deleted from the database, we have to poll it ourselves.
    let start = Instant::now();
    let timeout = Duration::from_secs(5);
    while start.elapsed() < timeout {
        if let Err(windows_service::Error::Winapi(e)) =
            service_manager.open_service(service_name, ServiceAccess::QUERY_STATUS)
        {
            if e.raw_os_error() == Some(1060i32) {
                return Ok(());
            }
        }
        sleep(Duration::from_secs(1));
    }
    Ok(())
}
