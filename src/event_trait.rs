use std::any::Any;
use variadics_please::all_tuples;

/// A group of events that can be subscribed to
pub trait EventGroup {
    fn event_ids() -> Vec<std::any::TypeId>;
}

macro_rules! impl_event_group {
        ($($P:ident),*) => {
            impl <$($P: Any),*> EventGroup for ($($P,)*) {
                fn event_ids() -> Vec<std::any::TypeId> {
                    vec![
                        $(std::any::TypeId::of::<$P>()),*
                    ]
                }
            }
        };
    }

all_tuples!(impl_event_group, 0, 15, P);
