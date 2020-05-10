use crate::{Node, RenderContext};
use async_trait::async_trait;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::UnwrapThrowExt;

/// A trait for any component that can be rendered to HTML.
///
/// Takes a shared reference to `self` and generates the virtual DOM that
/// represents its rendered HTML.
///
/// ## `Bump` Allocation
///
/// `Render` implementations can use the `Bump` inside the provided
/// `RenderContext` for very fast allocation for anything that needs to be
/// temporarily allocated during rendering.
///
/// ## Example
///
/// ```no_run
/// use dodrio::{Node, Render, RenderContext};
///
/// pub struct MyComponent;
///
/// impl<'a> Render<'a> for MyComponent {
///     fn render(&self, cx: &mut RenderContext<'a>) -> Node<'a> {
///         use dodrio::builder::*;
///
///         p(&cx)
///             .children([
///                 text("This is "),
///                 strong(&cx).children([text("my component")]).finish(),
///                 text(" rendered!"),
///             ])
///             .finish()
///     }
/// }
/// ```
pub trait Render<'a> {
    /// Render `self` as a virtual DOM. Use the given context's `Bump` for
    /// temporary allocations.
    fn render(&self, cx: &mut RenderContext<'a>) -> Node<'a>;
}

impl<'a, 'r, R> Render<'a> for &'r R
where
    R: Render<'a>,
{
    fn render(&self, cx: &mut RenderContext<'a>) -> Node<'a> {
        (**self).render(cx)
    }
}

impl<'a, R> Render<'a> for Rc<R>
where
    R: Render<'a>,
{
    fn render(&self, cx: &mut RenderContext<'a>) -> Node<'a> {
        (**self).render(cx)
    }
}

/// A trait for any component that can be async rendered to HTML.
///
/// Takes a shared reference to `self` and generates the virtual DOM that
/// represents its rendered HTML.
#[async_trait(?Send)]
pub trait AsyncRender<'a> {
    /// Render `self` as a virtual DOM. Use the given context's `Bump` for
    /// temporary allocations.
    async fn render(&self, cx: Rc<RefCell<RenderContext<'a>>>) -> Node<'a>;
}

#[async_trait(?Send)]
impl<'a, R: Render<'a>> AsyncRender<'a> for R {
    async fn render(&self, cx: Rc<RefCell<RenderContext<'a>>>) -> Node<'a> {
        let node = {
            let mut cx = cx.borrow_mut();
            Render::render(self, &mut *cx)
        };
        node
    }
}

/// A `RootRender` is a render component that can be the root rendering component
/// mounted to a virtual DOM.
///
/// In addition to rendering, it must also be `'static` so that it can be owned
/// by the virtual DOM and `Any` so that it can be downcast to its concrete type
/// by event listener callbacks.
///
/// You do not need to implement this trait by hand: there is a blanket
/// implementation for all `Render` types that fulfill the `RootRender`
/// requirements.
pub trait RootRender: Any + for<'a> AsyncRender<'a> {
    /// Get this `&RootRender` trait object as an `&Any` trait object reference.
    fn as_any(&self) -> &dyn Any;

    /// Get this `&mut RootRender` trait object as an `&mut Any` trait object
    /// reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> RootRender for T
where
    T: Any + for<'a> AsyncRender<'a>,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn RootRender {
    /// Downcast this shared `&dyn RootRender` trait object reference to its
    /// underlying concrete type.
    ///
    /// # Panics
    ///
    /// Panics if this virtual DOM's root rendering component is not an `R`
    /// instance.
    pub fn unwrap_ref<R>(&self) -> &R
    where
        R: RootRender,
    {
        self.as_any()
            .downcast_ref::<R>()
            .expect_throw("bad `RootRender::unwrap_ref` call")
    }

    /// Downcast this exclusive `&mut dyn RootRender` trait object reference to
    /// its underlying concrete type.
    ///
    /// # Panics
    ///
    /// Panics if this virtual DOM's root rendering component is not an `R`
    /// instance.
    pub fn unwrap_mut<R>(&mut self) -> &mut R
    where
        R: RootRender,
    {
        self.as_any_mut()
            .downcast_mut::<R>()
            .expect_throw("bad `RootRender::unwrap_ref` call")
    }
}

use std::future::Future;
use std::pin::Pin;

#[async_trait(?Send)]
impl<'a> AsyncRender<'a>
    for RefCell<
        genawaiter::rc::Gen<
            Option<Node<'a>>,
            Rc<RefCell<RenderContext<'a>>>,
            Pin<Box<dyn Future<Output = ()> + 'static>>,
        >,
    >
{
    async fn render(&self, cx: Rc<RefCell<RenderContext<'a>>>) -> Node<'a> {
        use genawaiter::GeneratorState;
        let gen = &mut *self.borrow_mut();
        // TODO: use async version
        // https://github.com/whatisaphone/genawaiter/issues/21
        match gen.resume_with(cx.clone()) {
            GeneratorState::Yielded(Some(node)) => node,
            GeneratorState::Yielded(None) => self.render(cx).await,
            GeneratorState::Complete(_) => panic!("my watch has ended"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn render_is_object_safe() {
        #[allow(dead_code)]
        fn takes_dyn_render(_: &dyn super::Render) {}
    }

    #[test]
    fn root_render_is_object_safe() {
        #[allow(dead_code)]
        fn takes_dyn_render(_: &dyn super::RootRender) {}
    }

    #[test]
    fn render_bump_scoped_child() {
        use crate::{builder::*, bumpalo::collections::String, Node, Render, RenderContext};

        struct Child<'a> {
            name: &'a str,
        }

        impl<'a> Render<'a> for Child<'a> {
            fn render(&self, _cx: &mut RenderContext<'a>) -> Node<'a> {
                text(self.name)
            }
        }

        struct Parent;

        impl<'a> Render<'a> for Parent {
            fn render(&self, cx: &mut RenderContext<'a>) -> Node<'a> {
                let child_name = String::from_str_in("child", cx.bump).into_bump_str();

                div(&cx)
                    .children([Child { name: child_name }.render(cx)])
                    .finish()
            }
        }
    }
}
