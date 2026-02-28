use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Id<T> {
    pub value: i64,
    _phantom: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(value: i64) -> Self {
        Self { value, _phantom: PhantomData }
    }
}

impl<T> From<i64> for Id<T> {
    fn from(id: i64) -> Self {
        Self::new(id)
    }
}

impl<T> From<Id<T>> for i64 {
    fn from(id: Id<T>) -> Self {
        id.value
    }
}

pub trait IdGenerate {
    fn generate_i64(&self) -> i64;

    fn generate<T>(&self) -> Id<T> {
        Id::new(self.generate_i64())
    }
}
