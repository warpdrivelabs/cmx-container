//! WASM 内存管理模块
//!
//! 提供 WASM 线性内存的分配和释放功能，
//! 用于 WASM 与宿主环境之间的数据传递。

use std::alloc::{alloc as std_alloc, dealloc as std_dealloc, Layout};

/// 分配 WASM 线性内存
///
/// 在 WASM 堆上分配指定大小的内存，返回指针。
/// 宿主环境可以通过此指针写入数据。
///
/// # 参数
/// * `size` - 要分配的字节数
///
/// # 返回值
/// 返回分配内存的起始指针（作为 i32）
///
/// # 安全性
/// 调用者必须确保在适当的时候调用 `dealloc` 释放内存
#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    let size = size as usize;
    if size == 0 {
        return 0;
    }
    
    let layout = match Layout::from_size_align(size, 1) {
        Ok(layout) => layout,
        Err(_) => return 0,
    };
    
    unsafe {
        let ptr = std_alloc(layout);
        if ptr.is_null() {
            return 0;
        }
        ptr as i32
    }
}

/// 释放 WASM 线性内存
///
/// 释放之前通过 `alloc` 分配的内存。
///
/// # 参数
/// * `ptr` - 内存起始指针
/// * `size` - 内存大小（必须与分配时相同）
///
/// # 安全性
/// - `ptr` 必须是通过 `alloc` 返回的有效指针
/// - `size` 必须与分配时的大小相同
/// - 不要重复释放同一块内存
#[no_mangle]
pub extern "C" fn dealloc(ptr: i32, size: i32) {
    let ptr = ptr as *mut u8;
    let size = size as usize;
    
    if ptr.is_null() || size == 0 {
        return;
    }
    
    let layout = match Layout::from_size_align(size, 1) {
        Ok(layout) => layout,
        Err(_) => return,
    };
    
    unsafe {
        std_dealloc(ptr, layout);
    }
}
