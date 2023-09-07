use super::*;

pub struct DummyInfoProvider;

impl WmInfoProvider for DummyInfoProvider {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
