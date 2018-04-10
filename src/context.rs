//! Module for API context management.
//!
//! This module defines traits and structs that can be used  to manage
//! contextual data related to a request, as it is passed through a series of
//! hyper services.

use auth::{Authorization, AuthData};
use std::marker::Sized;
use super::XSpanIdString;

/// Defines methods for accessing, modifying, adding and removing the data stored
/// in a context. Used to specify the requirements that a hyper service makes on
/// a generic context type that it receives with a request, e.g.
///
/// ```rust
/// # extern crate hyper;
/// # extern crate swagger;
/// # extern crate futures;
/// #
/// # use swagger::context::*;
/// # use futures::future::{Future, ok};
/// # use std::marker::PhantomData;
/// #
/// # struct MyItem;
/// # fn do_something_with_my_item(item: &MyItem) {}
/// #
/// struct MyService<C> {
///     marker: PhantomData<C>,
/// }
///
/// impl<C> hyper::server::Service for MyService<C>
///     where C: Has<MyItem>,
/// {
///     type Request = (hyper::Request, C);
///     type Response = hyper::Response;
///     type Error = hyper::Error;
///     type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;
///     fn call(&self, (req, context) : Self::Request) -> Self::Future {
///         do_something_with_my_item(Has::<MyItem>::get(&context));
///         Box::new(ok(hyper::Response::new()))
///     }
/// }
///
/// # fn main() {}
/// ```
pub trait Has<T> {
    /// Get an immutable reference to the value.
    fn get(&self) -> &T;
    /// Split into a the value and the remainder.
    fn get_mut(&mut self) -> &mut T;
    /// Set the value.
    fn set(&mut self, value: T);
}

pub trait Pop<T> {
    type Result;
    fn pop(self) -> (T, Self::Result);
}

pub trait Push<T> {
    type Result;
    fn push(self, T) -> Self::Result;
}

/// Defines a struct that can be used to build up contexts recursively by
/// adding one item to the context at a time. The first argument is the name
/// of the newly defined context struct, and subsequent arguments are the types
/// that can be stored in contexts built using this struct.
///
/// A cons list built using the generated context type will implement Has<T>
/// for each type T that appears in the list, provided that the list only
/// contains the types that were passed to the macro invocation after the context
/// type name.
///
/// E.g.
///
/// ```rust
/// # #[macro_use] extern crate swagger;
/// # use swagger::Has;
///
/// struct MyType1;
/// struct MyType2;
/// struct MyType3;
/// struct MyType4;
///
/// new_context_type!(MyContext, MyType1, MyType2, MyType3);
///
/// fn use_has_my_type_1<T: Has<MyType1>> (_: &T) {}
/// fn use_has_my_type_2<T: Has<MyType2>> (_: &T) {}
/// fn use_has_my_type_3<T: Has<MyType3>> (_: &T) {}
/// fn use_has_my_type_4<T: Has<MyType4>> (_: &T) {}
///
/// type ExampleContext = MyContext<MyType1, MyContext<MyType2, MyContext<MyType3, ()>>>;
/// type BadContext = MyContext<MyType1, MyContext<MyType4, ()>>;
///
/// fn main() {
///     let context: ExampleContext = MyContext::construct(
///         MyType1{},
///         MyContext::construct(
///             MyType2{},
///             MyContext::construct(MyType3{}, ())
///         )
///     );
///     use_has_my_type_1(&context);
///     use_has_my_type_2(&context);
///     use_has_my_type_3(&context);
///
///     let bad_context: BadContext = MyContext::construct(
///         MyType1{},
///         MyContext::construct(MyType4{}, ())
///     );
///
///     // will not work
///     // use_has_my_type_4(&bad_context);
///
/// }
/// ```
///
/// will define a new struct `MyContext<C, T>`, which implements:
/// - `Has<T>`,
/// - `ExtendsWith<Inner=C, Ext=T>`,
/// - `Has<S>` whenever `S` is one of `MyType1`, `MyType2` or `MyType3`, AND
///   `C` implements `Has<S>`.
///
/// See the `context_tests` module for more usage examples.
#[macro_export]
macro_rules! new_context_type {
    ($context_name:ident, $empty_context_name:ident, $($types:ty),+ ) => {

        /// Wrapper type for building up contexts recursively, adding one item
        /// to the context at a time.
        #[derive(Debug, Clone, Default)]
        pub struct $context_name<T, C> {
            head: T,
            tail: C,
        }

        #[derive(Debug, Clone, Default)]
        pub struct $empty_context_name;

        impl<U> $crate::Push<U> for $empty_context_name {
            type Result = $context_name<U, Self>;
            fn push(self, item: U) -> Self::Result {
                $context_name{head: item, tail: Self::default()}
            }
        }

        impl<T, C> $crate::Has<T> for $context_name<T, C> {
            fn set(&mut self, item: T) {
                self.head = item;
            }

            fn get(&self) -> &T {
                &self.head
            }

            fn get_mut(&mut self) -> &mut T {
                &mut self.head
            }
        }

        impl<T, C> $crate::Pop<T> for $context_name<T, C> {
            type Result = C;
            fn pop(self) -> (T, Self::Result) {
                (self.head, self.tail)
            }
        }

        impl<C, T, U> $crate::Push<U> for $context_name<T, C> {
            type Result = $context_name<U, Self>;
            fn push(self, item: U) -> Self::Result {
                $context_name{head: item, tail: self}
            }
        }

        new_context_type!(impl extend_has $context_name, $empty_context_name, $($types),+);
    };
    (impl extend_has $context_name:ident, $empty_context_name:ident, $head:ty, $($tail:ty),+ ) => {
        new_context_type!(impl extend_has_helper $context_name, $empty_context_name, $head, $($tail),+);
        new_context_type!(impl extend_has $context_name, $empty_context_name, $($tail),+);
    };
    (impl extend_has $context_name:ident, $empty_context_name:ident, $head:ty) => {};
    (impl extend_has_helper $context_name:ident , $empty_context_name:ident, $type:ty, $($types:ty),+ ) => {
        $(
            impl<C: $crate::Has<$type>> $crate::Has<$type> for $context_name<$types, C> {
                fn set(&mut self, item: $type) {
                    self.tail.set(item);
                }

                fn get(&self) -> &$type {
                    self.tail.get()
                }

                fn get_mut(&mut self) -> &mut $type {
                    self.tail.get_mut()
                }
            }

            impl<C: $crate::Has<$types>> $crate::Has<$types> for $context_name<$type, C> {
                fn set(&mut self, item: $types) {
                    self.tail.set(item);
                }

                fn get(&self) -> &$types {
                    self.tail.get()
                }

                fn get_mut(&mut self) -> &mut $types {
                    self.tail.get_mut()
                }
            }

            impl<C> $crate::Pop<$type> for $context_name<$types, C> where C: Pop<$type> {
                type Result = $context_name<$types, C::Result>;
                fn pop(self) -> ($type, Self::Result) {
                    let (value, tail) = self.tail.pop();
                    (value, $context_name{ head: self.head, tail: tail})
                }
            }

            impl<C> $crate::Pop<$types> for $context_name<$type, C> where C: Pop<$types> {
                type Result = $context_name<$type, C::Result>;
                fn pop(self) -> ($types, Self::Result) {
                    let (value, tail) = self.tail.pop();
                    (value, $context_name{ head: self.head, tail: tail})
                }
            }
        )+
    };
}

/// Create a default context type to export.
new_context_type!(Context, EmpContext, XSpanIdString, Option<AuthData>, Option<Authorization>);

/// Macro for easily defining context types. The first argument should be a
/// context type created with `new_context_type!` and subsequent arguments are the
/// types to be stored in the context, with the outermost first.
#[macro_export]
macro_rules! make_context_ty {
    ($context_name:ident, $empty_context_name:ident, $type:ty $(, $types:ty)* $(,)* ) => {
        $context_name<$type, make_context_ty!($context_name, $empty_context_name, $($types),*)>
    };
    ($context_name:ident, $empty_context_name:ident $(,)* ) => {
        $empty_context_name
    };
}

/// Macro for easily defining context values. The first argument should be a
/// context type created with `new_context_type!` and subsequent arguments are the
/// values to be stored in the context, with the outermost first.
#[macro_export]
macro_rules! make_context {
    ($context_name:ident, $empty_context_name:ident, $value:expr $(, $values:expr)* $(,)*) => {
        make_context!($context_name, $empty_context_name, $($values),*).push($value)
    };
    ($context_name:ident, $empty_context_name:ident $(,)* ) => {
        $empty_context_name::default()
    };
}

/// Context wrapper, to bind an API with a context.
#[derive(Debug)]
pub struct ContextWrapper<'a, T: 'a, C> {
    api: &'a T,
    context: C,
}

impl<'a, T, C> ContextWrapper<'a, T, C> {
    /// Create a new ContextWrapper, binding the API and context.
    pub fn new(api: &'a T, context: C) -> ContextWrapper<'a, T, C> {
        ContextWrapper { api, context }
    }

    /// Borrows the API.
    pub fn api(&self) -> &T {
        self.api
    }

    /// Borrows the context.
    pub fn context(&self) -> &C {
        &self.context
    }
}

/// Trait to extend an API to make it easy to bind it to a context.
pub trait ContextWrapperExt<'a, C>
where
    Self: Sized,
{
    /// Binds this API to a context.
    fn with_context(self: &'a Self, context: C) -> ContextWrapper<'a, Self, C> {
        ContextWrapper::<Self, C>::new(self, context)
    }
}

#[cfg(test)]
mod context_tests {
    use hyper::server::{NewService, Service};
    use hyper::{Response, Request, Error, Method, Uri};
    use std::marker::PhantomData;
    use std::io;
    use std::str::FromStr;
    use futures::future::{Future, ok};
    use super::*;

    struct ContextItem1;
    struct ContextItem2;

    fn do_something_with_item_1(_: &ContextItem1) {}
    fn do_something_with_item_2(_: &ContextItem2) {}

    struct InnerService<C>
    where
        C: Has<ContextItem2>,
    {
        marker: PhantomData<C>,
    }

    impl<C> Service for InnerService<C>
    where
        C: Has<ContextItem2>,
    {
        type Request = (Request, C);
        type Response = Response;
        type Error = Error;
        type Future = Box<Future<Item = Response, Error = Error>>;
        fn call(&self, (_, context): Self::Request) -> Self::Future {
            do_something_with_item_2(Has::<ContextItem2>::get(&context));
            Box::new(ok(Response::new()))
        }
    }

    struct InnerNewService<C>
    where
        C: Has<ContextItem2>,
    {
        marker: PhantomData<C>,
    }

    impl<C> InnerNewService<C>
    where
        C: Has<ContextItem2>,
    {
        fn new() -> Self {
            InnerNewService { marker: PhantomData }
        }
    }

    impl<C> NewService for InnerNewService<C>
    where
        C: Has<ContextItem2>,
    {
        type Request = (Request, C);
        type Response = Response;
        type Error = Error;
        type Instance = InnerService<C>;
        fn new_service(&self) -> Result<Self::Instance, io::Error> {
            Ok(InnerService { marker: PhantomData })
        }
    }

    struct MiddleService<T, C>
    where
        C: Pop<ContextItem1>,
        C::Result : Push<ContextItem2>,
        T: Service<Request = (Request, <C::Result as Push<ContextItem2>>::Result)>,
    {
        inner: T,
        marker1: PhantomData<C>,
    }

    impl<T, C> Service for MiddleService<T, C>
    where
        C: Pop<ContextItem1>,
        C::Result : Push<ContextItem2>,
        T: Service<Request = (Request, <C::Result as Push<ContextItem2>>::Result)>,
    {
        type Request = (Request, C);
        type Response = T::Response;
        type Error = T::Error;
        type Future = T::Future;
        fn call(&self, (req, context): Self::Request) -> Self::Future {
            let (item, context) = context.pop();
            do_something_with_item_1(&item);
            let context = context.push(ContextItem2 {});
            self.inner.call((req, context))
        }
    }

    struct MiddleNewService<T, C>
    where
        C: Pop<ContextItem1>,
        C::Result : Push<ContextItem2>,
        T: NewService<Request = (Request, <C::Result as Push<ContextItem2>>::Result)>,
    {
        inner: T,
        marker1: PhantomData<C>,
    }

    impl<T, C> NewService for MiddleNewService<T, C>
    where
        C: Pop<ContextItem1>,
        C::Result : Push<ContextItem2>,
        T: NewService<Request = (Request, <C::Result as Push<ContextItem2>>::Result)>,
    {
        type Request = (Request, C);
        type Response = T::Response;
        type Error = T::Error;
        type Instance = MiddleService<T::Instance, C>;
        fn new_service(&self) -> Result<Self::Instance, io::Error> {
            self.inner.new_service().map(|s| {
                MiddleService {
                    inner: s,
                    marker1: PhantomData,
                }
            })
        }
    }

    impl<T, C> MiddleNewService<T, C>
    where
        C: Pop<ContextItem1>,
        C::Result : Push<ContextItem2>,
        T: NewService<Request = (Request, <C::Result as Push<ContextItem2>>::Result)>,
    {
        fn new(inner: T) -> Self {
            MiddleNewService {
                inner,
                marker1: PhantomData,
            }
        }
    }

    struct OuterService<T, C>
    where
        C: Default + Push<ContextItem1>,
        T: Service<Request = (Request, C::Result)>,
    {
        inner: T,
        marker: PhantomData<C>,
    }

    impl<T, C> Service for OuterService<T, C>
    where
        C: Default + Push<ContextItem1>,
        T: Service<Request = (Request, C::Result)>,
    {
        type Request = Request;
        type Response = T::Response;
        type Error = T::Error;
        type Future = T::Future;
        fn call(&self, req: Self::Request) -> Self::Future {
            let context = C::default().push(ContextItem1 {});
            self.inner.call((req, context))
        }
    }

    struct OuterNewService<T, C>
    where
        C: Default + Push<ContextItem1>,
        T: NewService<Request = (Request, C::Result)>,
    {
        inner: T,
        marker: PhantomData<C>,
    }

    impl<T, C> NewService for OuterNewService<T, C>
    where
        C: Default + Push<ContextItem1>,
        T: NewService<Request = (Request, C::Result)>,
    {
        type Request = Request;
        type Response = T::Response;
        type Error = T::Error;
        type Instance = OuterService<T::Instance, C>;
        fn new_service(&self) -> Result<Self::Instance, io::Error> {
            self.inner.new_service().map(|s| {
                OuterService {
                    inner: s,
                    marker: PhantomData,
                }
            })
        }
    }

    impl<T, C> OuterNewService<T, C>
    where
        C: Default + Push<ContextItem1>,
        T: NewService<Request = (Request, C::Result)>,
    {
        fn new(inner: T) -> Self {
            OuterNewService {
                inner,
                marker: PhantomData,
            }
        }
    }

    new_context_type!(MyContext, MyEmptyContext, ContextItem1, ContextItem2);


    #[test]
    fn send_request() {

        let new_service =
            OuterNewService::<_, MyEmptyContext>::new(MiddleNewService::new(InnerNewService::new()));

        let req = Request::new(Method::Post, Uri::from_str("127.0.0.1:80").unwrap());
        new_service
            .new_service()
            .expect("Failed to start new service")
            .call(req)
            .wait()
            .expect("Service::call returned an error");
    }
}
