(module
  (type (;0;) (func (result i32)))
  (export "foo" (func 0))
  (func (;0;) (type 0) (result i32)
    (local i32)
    i32.const 42
    local.set 0
    local.get 0
  )
)
