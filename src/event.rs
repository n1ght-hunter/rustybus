use std::{any::Any, sync::Arc};


/// An event that contains a boxed or arc'd value. \
/// if the value is boxed this means that the underlying type impls clone. \
/// if the value is arc'd this means that user chose to not use reference counting instead of cloning either because the type is not clonable or because the user wants to avoid cloning.
#[derive(Debug)]
pub enum Event {
    Boxed(Box<dyn Any + Send + Sync>),
    Arc(Arc<dyn Any + Send + Sync>),
}

impl Event {
    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(s: &(dyn Any + Send + Sync)) {
    ///     if let Some(string) = s.downcast_ref::<String>() {
    ///         println!("It's a string({}): '{}'", string.len(), string);
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// print_if_string(&0);
    /// print_if_string(&"cookie monster".to_string());
    /// ```
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match self {
            Event::Boxed(event) => event.downcast_ref::<T>(),
            Event::Arc(event) => event.downcast_ref::<T>(),
        }
    }

    /// Forwards to the method defined on the type `Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn is_string(s: &(dyn Any + Send + Sync)) {
    ///     if s.is::<String>() {
    ///         println!("It's a string!");
    ///     } else {
    ///         println!("Not a string...");
    ///     }
    /// }
    ///
    /// is_string(&0);
    /// is_string(&"cookie monster".to_string());
    /// ```
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        match self {
            Event::Boxed(event) => event.is::<T>(),
            Event::Arc(event) => event.is::<T>(),
        }
    }

    /// Forwards to the method defined on the type `Any`. \
    /// This method will not work on [`Event::Arc(_)`]. Use `downcast_ref` instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn modify_if_u32(s: &mut (dyn Any + Send + Sync)) {
    ///     if let Some(num) = s.downcast_mut::<u32>() {
    ///         *num = 42;
    ///     }
    /// }
    ///
    /// let mut x = 10u32;
    /// let mut s = "starlord".to_string();
    ///
    /// modify_if_u32(&mut x);
    /// modify_if_u32(&mut s);
    ///
    /// assert_eq!(x, 42);
    /// assert_eq!(&s, "starlord");
    /// ```
    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        match self {
            Event::Boxed(event) => event.downcast_mut::<T>(),
            Event::Arc(_) => None,
        }
    }
}
