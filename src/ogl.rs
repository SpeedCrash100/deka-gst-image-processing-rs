mod glsobel;

use crate::glib;

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    glsobel::register(plugin)?;
    Ok(())
}
