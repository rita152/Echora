//! AppKit drag payloads, including unsaved Screenshot thumbnails.
use cocoa::{
    appkit::{NSFilenamesPboardType, NSPasteboard},
    base::{id, nil},
    foundation::{NSArray, NSFastEnumeration, NSString},
};
use objc::{class, msg_send, sel, sel_impl};
use std::{ffi::CStr, path::PathBuf};

use crate::ns_string;

/// Register modern URLs, image-only drags and file promises as well as Finder's
/// legacy filename list. Screenshot does not have to publish a saved file.
pub(super) unsafe fn register(window: id) {
    unsafe {
        let types: id = msg_send![class!(NSMutableArray), array];
        for kind in [
            "NSFilenamesPboardType",
            "public.file-url",
            "public.png",
            "public.tiff",
        ] {
            let _: () = msg_send![types, addObject: ns_string(kind)];
        }
        let promises: id = msg_send![class!(NSFilePromiseReceiver), readableDraggedTypes];
        let _: () = msg_send![types, addObjectsFromArray: promises];
        let _: () = msg_send![window, registerForDraggedTypes: types];
    }
}

pub(super) unsafe fn paths(pasteboard: id) -> Vec<PathBuf> {
    unsafe {
        let mut paths = Vec::new();
        let classes = NSArray::arrayWithObject(nil, class!(NSURL) as *const _ as id);
        let urls: id = msg_send![pasteboard, readObjectsForClasses: classes options: nil];
        if urls != nil {
            for url in urls.iter() {
                let file: bool = msg_send![url, isFileURL];
                if file {
                    let bytes: *const std::ffi::c_char = msg_send![url, fileSystemRepresentation];
                    if !bytes.is_null() {
                        use std::os::unix::ffi::OsStrExt;
                        paths.push(PathBuf::from(std::ffi::OsStr::from_bytes(
                            CStr::from_ptr(bytes).to_bytes(),
                        )));
                    }
                }
            }
        }
        let filenames = NSPasteboard::propertyListForType(pasteboard, NSFilenamesPboardType);
        if filenames != nil {
            for file in filenames.iter() {
                let path = PathBuf::from(
                    CStr::from_ptr(NSString::UTF8String(file))
                        .to_string_lossy()
                        .into_owned(),
                );
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }
}

pub(super) unsafe fn promises(pasteboard: id) -> id {
    unsafe {
        let classes =
            NSArray::arrayWithObject(nil, class!(NSFilePromiseReceiver) as *const _ as id);
        msg_send![pasteboard, readObjectsForClasses: classes options: nil]
    }
}

pub(super) unsafe fn has_image(pasteboard: id) -> bool {
    unsafe {
        let types: id = msg_send![pasteboard, types];
        ["public.png", "public.tiff"].iter().any(|kind| {
            let found: bool = msg_send![types, containsObject: ns_string(kind)];
            found
        })
    }
}

pub(super) fn destination() -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir()
        .join("gpui-dropped-files")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Only materialize image bytes after a drop, never on hover. Normalize TIFF
/// to PNG so agent input and previews share the same supported image format.
pub(super) unsafe fn image(pasteboard: id) -> anyhow::Result<Option<PathBuf>> {
    unsafe {
        for kind in ["public.png", "public.tiff"] {
            let data: id = msg_send![pasteboard, dataForType: ns_string(kind)];
            if data == nil {
                continue;
            }
            let length: usize = msg_send![data, length];
            let bytes: *const u8 = msg_send![data, bytes];
            if bytes.is_null() || length == 0 {
                continue;
            }
            let bytes = std::slice::from_raw_parts(bytes, length);
            let image = image::load_from_memory(bytes)?;
            let path = destination()?.join("Screenshot.png");
            image.save_with_format(&path, image::ImageFormat::Png)?;
            return Ok(Some(path));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cocoa::foundation::NSData;
    use objc::rc::autoreleasepool;

    #[test]
    fn modern_file_urls_preserve_unicode_and_ignore_web_urls() {
        autoreleasepool(|| unsafe {
            let board = NSPasteboard::pasteboardWithUniqueName(nil);
            let file: id =
                msg_send![class!(NSURL), fileURLWithPath: ns_string("/tmp/截图 100%.png")];
            let web: id =
                msg_send![class!(NSURL), URLWithString: ns_string("https://example.com/a.png")];
            let objects = NSArray::arrayWithObjects(nil, &[file, web]);
            let _: bool = msg_send![board, writeObjects: objects];
            assert_eq!(paths(board), vec![PathBuf::from("/tmp/截图 100%.png")]);
        });
    }

    #[test]
    fn image_only_drop_materializes_png_and_tiff_as_png() {
        for (kind, format) in [
            ("public.png", image::ImageFormat::Png),
            ("public.tiff", image::ImageFormat::Tiff),
        ] {
            autoreleasepool(|| unsafe {
                let board = NSPasteboard::pasteboardWithUniqueName(nil);
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::new_rgba8(12, 8)
                    .write_to(&mut bytes, format)
                    .unwrap();
                let bytes = bytes.into_inner();
                let data =
                    NSData::dataWithBytes_length_(nil, bytes.as_ptr().cast(), bytes.len() as u64);
                let _: bool = msg_send![board, setData: data forType: ns_string(kind)];
                assert!(paths(board).is_empty());
                assert!(has_image(board));
                let path = image(board).unwrap().unwrap();
                assert_eq!(image::image_dimensions(&path).unwrap(), (12, 8));
                assert_eq!(
                    image::guess_format(&std::fs::read(&path).unwrap()).unwrap(),
                    image::ImageFormat::Png
                );
                std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
            });
        }
    }

    #[test]
    fn text_drag_is_not_an_image_or_file() {
        autoreleasepool(|| unsafe {
            let board = NSPasteboard::pasteboardWithUniqueName(nil);
            let _: bool = msg_send![board, setString: ns_string("hello") forType: ns_string("public.utf8-plain-text")];
            assert!(paths(board).is_empty());
            assert!(!has_image(board));
            assert!(image(board).unwrap().is_none());
        });
    }
}
