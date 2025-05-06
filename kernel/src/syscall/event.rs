#[macro_export]
macro_rules! generate_event_syscall {
    (
        $cap_type:expr,
        $event_type_name:expr,
        $event_name:expr,
        $required_flags:expr, 
        $add_listener:expr
    ) => {
        paste::paste! {
            pub fn [<$cap_type _handle_ $event_name _sync>](
                options: u32,
                cap_id: usize,
                timeout: usize
            ) -> KResult<<$event_type_name as sys::EventSyncReturn>::SyncReturn> {
                let weak_auto_destroy = $crate::syscall::options_weak_autodestroy(options);
                let flags = sys::HandleEventSyncFlags::from_bits_truncate(options);

                let post_switch_action = if flags.contains(sys::HandleEventSyncFlags::TIMEOUT) {
                    $crate::sched::PostSwitchAction::SetTimeout(timeout as u64)
                } else {
                    $crate::sched::PostSwitchAction::None
                };

                let _int_disable = $crate::arch::x64::IntDisable::new();

                {
                    let capability = $crate::cap::capability_space::CapabilitySpace::current()
                        .[<get_ $cap_type _with_perms>](cap_id, $required_flags, weak_auto_destroy)?
                        .into_inner();

                    let current_thread = $crate::gs_data::cpu_local_data().current_thread();
                    let thread_ref = $crate::sched::ThreadRef::future_ref(&current_thread);
                    let listener = $crate::event::BroadcastEventListener::Thread(thread_ref);

                    $add_listener(&capability, listener)?;
                }
                // all reference counted objects should be dropped here

                $crate::sched::switch_current_thread_to(
                    $crate::sched::ThreadState::Suspended,
                    _int_disable,
                    post_switch_action,
                    false,
                ).unwrap();

                let _int_disable = $crate::arch::x64::IntDisable::new();

                let wake_reason = $crate::gs_data::cpu_local_data().current_thread().wake_reason();

                match wake_reason {
                    $crate::sched::WakeReason::Timeout => Err(SysErr::OkTimeout),
                    $crate::sched::WakeReason::EventRecieved(sys::EventData::$event_type_name(event_data)) =>
                        Ok(sys::EventSyncReturn::as_sync_return(&event_data)),
                    // this should not happen
                    _ => unreachable!(),
                }
            }

            pub fn [<$cap_type _handle_ $event_name _async>](
                options: u32,
                cap_id: usize,
                event_pool_id: usize,
                event_id: usize
            ) -> KResult<()> {
                let weak_auto_destroy = $crate::syscall::options_weak_autodestroy(options);
                let flags = sys::HandleEventAsyncFlags::from_bits_truncate(options);
                let event_id = sys::EventId::from_u64(event_id as u64);

                let _int_disable = $crate::arch::x64::IntDisable::new();

                let cspace = $crate::cap::capability_space::CapabilitySpace::current();

                let capability = cspace
                    .[<get_ $cap_type _with_perms>](cap_id, $required_flags, weak_auto_destroy)?
                    .into_inner();

                let event_pool = cspace
                    .get_event_pool_with_perms(event_pool_id, CapFlags::WRITE, weak_auto_destroy)?
                    .into_inner();

                let event_pool_listener = $crate::event::EventPoolListenerRef {
                    event_pool: $crate::container::Arc::downgrade(&event_pool),
                    event_id,
                };
                let listener = $crate::event::BroadcastEventListener::EventPool {
                    event_pool: event_pool_listener,
                    auto_reque: flags.contains(sys::HandleEventAsyncFlags::AUTO_REQUE),
                };

                $add_listener(&capability, listener)
            }
        }
    };
}