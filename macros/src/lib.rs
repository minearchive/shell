#[macro_export]
macro_rules! boxed {
    ($($e:expr),* $(,)?) => {
        vec![$(Box::new($e)),*]
    };
}

#[cfg(test)]
mod tests {

    #[test]
    fn macro_check() {
        let default = vec![Box::new(1), Box::new(2), Box::new(3)];
        let macros = boxed!(1, 2, 3);

        assert_eq!(default, macros)
    }
}
