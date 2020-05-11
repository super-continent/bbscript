#[macro_export]
macro_rules! verbose {
    ($msg:expr, $var:ident) => {
        if $var == true {
            $msg
        };
    };
}
