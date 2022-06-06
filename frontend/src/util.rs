
    // Helper function for annoying conversion from Closure to Function
    fn action_to_func_ref<T: ?Sized>(action: &Option<Closure<T>>) -> &js_sys::Function {
        action.as_ref().unwrap().as_ref().unchecked_ref()
    }

