(module
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i32)))
  (export "result_u64" (func 0))
  (export "result_u8" (func 1))
  (func (;0;) (type 0) (result i64)
    i64.const 42
  )
  (func (;1;) (type 1) (result i32)
    i32.const 7
  )
)
