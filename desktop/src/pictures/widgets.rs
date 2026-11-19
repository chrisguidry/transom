//! A party's picture as the lists and pages draw it: a circle with the
//! picture, or with its initials on a quiet ground when it has none, and
//! the badge of a message's disposition on its upper right.

use std::rc::Rc;

use adw::prelude::*;

use super::{badge, initials};
use crate::exchange::Disposition;
use crate::window::Shell;

/// The picture of the party called `name`, `size` pixels across. The
/// initials show at once, and the picture at `url` takes their place
/// when it has been fetched.
pub fn avatar(
    shell: &Rc<Shell>,
    name: &str,
    url: Option<&str>,
    size: i32,
    disposition: Option<Disposition>,
) -> gtk::Overlay {
    let letters = gtk::Label::builder()
        .label(initials(name))
        .width_request(size)
        .height_request(size)
        .build();
    letters.add_css_class("avatar");
    letters.add_css_class("initials");
    let overlay = gtk::Overlay::builder()
        .child(&letters)
        .valign(gtk::Align::Start)
        .build();
    if let Some((class, glyph)) = disposition.and_then(badge) {
        let mark = gtk::Label::builder()
            .label(glyph)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .build();
        mark.add_css_class("badge");
        mark.add_css_class(class);
        overlay.add_overlay(&mark);
        overlay.set_clip_overlay(&mark, false);
    }
    let Some(url) = url else { return overlay };
    if let Some(texture) = shell.textures.borrow().get(url) {
        show(&overlay, texture, size);
        return overlay;
    }
    if let Some(pictures) = shell.snapshot().pictures.clone() {
        let (asked, kept) = (url.to_owned(), url.to_owned());
        let (shown, keeping) = (overlay.clone(), Rc::downgrade(shell));
        // The picture is decoded as a texture, which reads WebP as well
        // as PNG and JPEG; an image widget given the file's name draws
        // nothing for a WebP. It is decoded off the window's thread. A
        // file that is no picture leaves the initials.
        let decode = async move {
            let file = pictures.picture(&asked).await?;
            tokio::task::spawn_blocking(move || gtk::gdk::Texture::from_filename(file).ok())
                .await
                .ok()?
        };
        shell.run(decode, move |texture| {
            let Some(texture) = texture else { return };
            show(&shown, &texture, size);
            if let Some(shell) = keeping.upgrade() {
                shell.textures.borrow_mut().insert(kept, texture);
            }
        });
    }
    overlay
}

/// Puts `texture` in `overlay` in place of the initials. An image is
/// drawn at its pixel size whatever the picture's own, and the frame's
/// corners clip it to a circle.
fn show(overlay: &gtk::Overlay, texture: &gtk::gdk::Texture, size: i32) {
    let picture = gtk::Image::from_paintable(Some(texture));
    picture.set_pixel_size(size);
    let frame = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .overflow(gtk::Overflow::Hidden)
        .build();
    frame.add_css_class("avatar");
    frame.append(&picture);
    overlay.set_child(Some(&frame));
}
