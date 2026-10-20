use dioxus::prelude::*;

#[component]
pub fn Keyed(identity: String, children: Element) -> Element {
    // Dioxus reconciles keys in sibling lists, not at a component's root.
    rsx! {
        for identity in [identity] {
            Fragment {
                key: "{identity}",
                {children.clone()}
            }
        }
        // Dioxus reconciles keys in sibling lists, not at a component's root.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, PartialEq, Props)]
    struct HarnessProps {
        control: Rc<RefCell<Option<Signal<u8>>>>,
        observed: Rc<RefCell<Vec<u8>>>,
    }
    #[allow(non_snake_case)]
    fn Harness(props: HarnessProps) -> Element {
        let identity = use_signal(|| 1_u8);
        *props.control.borrow_mut() = Some(identity);
        rsx! {
            Keyed {
                identity: identity().to_string(),
                Probe {
                    value: identity(),
                    observed: props.observed,
                }
            }
        }
    }
    #[component]
    fn Probe(value: u8, observed: Rc<RefCell<Vec<u8>>>) -> Element {
        let initial = use_signal(|| value);
        observed.borrow_mut().push(initial());
        rsx! {
            span {

                "{initial}"
            }
        }
    }

    #[test]
    fn changing_identity_drops_old_hook_state() {
        let control = Rc::new(RefCell::new(None));
        let observed = Rc::new(RefCell::new(Vec::new()));
        let mut dom = VirtualDom::new_with_props(
            Harness,
            HarnessProps {
                control: control.clone(),
                observed: observed.clone(),
            },
        );
        dom.rebuild_in_place();
        let mut identity = control.borrow().unwrap();
        dom.in_runtime(|| identity.set(2));
        dom.render_immediate_to_vec();
        assert_eq!(*observed.borrow(), vec![1, 2]);
    }
}
