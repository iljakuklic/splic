(module
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (param i64) (result i64)))
  (export "min_signed" (func 0))
  (export "max_u64" (func 1))
  (export "clamp_large" (func 2))
  (func (;0;) (type 0) (result i64)
    i64.const -9223372036854775808
  )
  (func (;1;) (type 1) (result i64)
    i64.const -1
  )
  (func (;2;) (type 2) (param i64) (result i64)
    (local i64 i64)
    local.get 0
    local.set 1
    block (result i64) ;; label = @1
      block ;; label = @2
        local.get 1
        i64.const -9223372036854775808
        i64.ne
        br_if 0 (;@2;)
        i64.const 0
        br 1 (;@1;)
      end
      block ;; label = @2
        local.get 1
        i64.const -1
        i64.ne
        br_if 0 (;@2;)
        i64.const 1
        br 1 (;@1;)
      end
      local.get 1
      local.set 2
      local.get 2
    end
  )
)
