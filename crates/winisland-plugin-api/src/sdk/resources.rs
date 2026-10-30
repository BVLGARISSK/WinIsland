use std::ffi::c_void;

use super::{
    ContextApi, Error, Host, HostStateApi, ImageApi, ImageHandle, LyricsApi, MediaApi, SettingsApi,
    StoreApi, TextApi, copy_fixed, success,
};
use crate::abi::{
    self, ContextApiV2, HostStateApiV2, ImageApiV2, LyricsTransformApiV2, MediaApiV2, PluginStatus,
    SettingsApiV2, StoreApiV2, TextApiV2,
};
use crate::types::v2::context::{ContextDataV2, HostStateV2, MediaSourceDataV2};
use crate::types::v2::lyrics::{
    LYRICS_TRANSFORMER_FLAG_SECONDARY, LyricsTextV2, LyricsTransformerDataV2,
};
use crate::types::v2::settings::{SETTINGS_ITEM_SECTION, SettingsItemV2, SettingsPageDataV2};
use crate::types::v2::{ByteSlice, ImageId, ResourceId, TextMetricsV2, TextStyleV2, Utf8Slice};

enum ResourceKind {
    Context,
    Media,
    Lyrics,
    Settings,
}

type Transform = dyn Fn(&str) -> String + Send + Sync;
type SecondaryTransform = dyn Fn(&LyricsLine<'_>) -> String + Send + Sync;

/// One lyric line passed to a secondary-text transformer, with the whole song for context.
pub struct LyricsLine<'a> {
    pub text: &'a str,
    pub time_ms: u64,
    pub index: u32,
    pub count: u32,
    /// Every line of the song joined by `\n`.
    pub song: &'a str,
}

pub struct Resource {
    host: Host,
    id: ResourceId,
    kind: ResourceKind,
    callback: Option<Box<dyn std::any::Any + Send + Sync>>,
}

impl Resource {
    pub fn id(&self) -> ResourceId {
        self.id
    }
}

impl Drop for Resource {
    fn drop(&mut self) {
        let result = match self.kind {
            ResourceKind::Context => self
                .host
                .query::<ContextApiV2>(abi::IFACE_CONTEXT)
                .ok()
                .and_then(|table| {
                    table.release.map(|release| {
                        // SAFETY: The resource remains owned by this plugin until release returns.
                        unsafe { release(table.prefix.context, self.host.token, self.id) }
                    })
                }),
            ResourceKind::Media => self
                .host
                .query::<MediaApiV2>(abi::IFACE_MEDIA)
                .ok()
                .and_then(|table| {
                    table.release.map(|release| {
                        // SAFETY: The resource remains owned by this plugin until release returns.
                        unsafe { release(table.prefix.context, self.host.token, self.id) }
                    })
                }),
            ResourceKind::Lyrics => self
                .host
                .query::<LyricsTransformApiV2>(abi::IFACE_LYRICS_TRANSFORM)
                .ok()
                .and_then(|table| {
                    table.release.map(|release| {
                        // SAFETY: The host stops callbacks before release returns.
                        unsafe { release(table.prefix.context, self.host.token, self.id) }
                    })
                }),
            ResourceKind::Settings => self
                .host
                .query::<SettingsApiV2>(abi::IFACE_SETTINGS)
                .ok()
                .and_then(|table| {
                    table.release.map(|release| {
                        // SAFETY: The resource remains owned by this plugin until release returns.
                        unsafe { release(table.prefix.context, self.host.token, self.id) }
                    })
                }),
        };
        if !matches!(result, Some(PluginStatus::Ok | PluginStatus::StaleHandle))
            && let Some(callback) = self.callback.take()
        {
            std::mem::forget(callback);
        }
    }
}

impl ContextApi {
    pub fn create(&self, title: &str, body: &str) -> Result<Resource, Error> {
        let table = self._host.query::<ContextApiV2>(abi::IFACE_CONTEXT)?;
        let create = table
            .create
            .ok_or(Error::MissingFunction("context.create"))?;
        let mut data = ContextDataV2::default();
        copy_fixed(&mut data.title, title);
        copy_fixed(&mut data.body, body);
        let mut id = ResourceId::INVALID;
        // SAFETY: The data and output id live until this synchronous call returns.
        success(unsafe { create(table.prefix.context, self._host.token, &data, &mut id) })?;
        Ok(Resource {
            host: self._host.clone(),
            id,
            kind: ResourceKind::Context,
            callback: None,
        })
    }
}

impl MediaApi {
    pub fn create_source(&self, title: &str, artist: &str) -> Result<Resource, Error> {
        let table = self.0.query::<MediaApiV2>(abi::IFACE_MEDIA)?;
        let create = table.create.ok_or(Error::MissingFunction("media.create"))?;
        let mut data = MediaSourceDataV2::default();
        copy_fixed(&mut data.title, title);
        copy_fixed(&mut data.artist, artist);
        let mut id = ResourceId::INVALID;
        // SAFETY: The host copies the source data during this call.
        success(unsafe { create(table.prefix.context, self.0.token, &data, &mut id) })?;
        Ok(Resource {
            host: self.0.clone(),
            id,
            kind: ResourceKind::Media,
            callback: None,
        })
    }
}

impl HostStateApi {
    pub fn get(&self) -> Result<HostStateV2, Error> {
        let table = self._host.query::<HostStateApiV2>(abi::IFACE_HOST_STATE)?;
        let get = table.get.ok_or(Error::MissingFunction("host_state.get"))?;
        let mut state = HostStateV2::default();
        // SAFETY: The output snapshot lives until the synchronous call returns.
        success(unsafe { get(table.prefix.context, self._host.token, &mut state) })?;
        Ok(state)
    }
}

/// # Safety
/// A non-null `slice.ptr` must be readable for `slice.len` bytes while the result is used.
unsafe fn borrowed_text<'a>(slice: Utf8Slice) -> Option<&'a str> {
    if slice.len == 0 {
        return Some("");
    }
    if slice.ptr.is_null() {
        return None;
    }
    // SAFETY: The caller guarantees the non-null pointer is valid for the declared length.
    let bytes = unsafe { std::slice::from_raw_parts(slice.ptr, slice.len as usize) };
    std::str::from_utf8(bytes).ok()
}

unsafe extern "C" fn transform_line(
    callback_data: *mut c_void,
    _resource_id: ResourceId,
    input: *const LyricsTextV2,
    output: *mut u8,
    capacity: u32,
    out_len: *mut u32,
) -> PluginStatus {
    if callback_data.is_null() || input.is_null() || out_len.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: Every host supplies at least `struct_size`, and no reference to the whole struct is
    // formed.
    let size = unsafe { (&raw const (*input).struct_size).read() };
    // Hosts that predate the song fields pass only the prefix ending at `text`, so neither the
    // later fields nor a reference to the full struct may be used.
    if (size as usize) < std::mem::offset_of!(LyricsTextV2, text) + std::mem::size_of::<Utf8Slice>()
    {
        return PluginStatus::UnsupportedVersion;
    }
    // SAFETY: The checked size covers `text`, which is read through the raw pointer.
    let text = unsafe { (&raw const (*input).text).read() };
    // SAFETY: The host keeps the borrowed text valid for this callback.
    let Some(text) = (unsafe { borrowed_text(text) }) else {
        return PluginStatus::InvalidArgument;
    };
    // SAFETY: The callback pointer is the stable inner Box retained by Resource.
    let callback = unsafe { &*(callback_data as *const Box<Transform>) };
    let Ok(value) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(text)))
    else {
        return PluginStatus::Internal;
    };
    // SAFETY: The host provides a valid out_len pointer and `capacity` writable output bytes.
    unsafe { write_output(&value, output, capacity, out_len) }
}

unsafe extern "C" fn transform_secondary(
    callback_data: *mut c_void,
    _resource_id: ResourceId,
    input: *const LyricsTextV2,
    output: *mut u8,
    capacity: u32,
    out_len: *mut u32,
) -> PluginStatus {
    if callback_data.is_null() || input.is_null() || out_len.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host keeps the input prefix alive for this callback.
    let size = unsafe { (*input).struct_size };
    if (size as usize) < std::mem::size_of::<LyricsTextV2>() {
        return PluginStatus::UnsupportedVersion;
    }
    // SAFETY: The checked size covers every field read below.
    let input = unsafe { &*input };
    // SAFETY: The host keeps both borrowed strings valid for this callback.
    let (Some(text), Some(song)) =
        (unsafe { (borrowed_text(input.text), borrowed_text(input.song)) })
    else {
        return PluginStatus::InvalidArgument;
    };
    let line = LyricsLine {
        text,
        time_ms: input.line_time_ms,
        index: input.line_index,
        count: input.line_count,
        song,
    };
    // SAFETY: The callback pointer is the stable inner Box retained by Resource.
    let callback = unsafe { &*(callback_data as *const Box<SecondaryTransform>) };
    let Ok(value) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&line)))
    else {
        return PluginStatus::Internal;
    };
    // SAFETY: The host provides a valid out_len pointer and `capacity` writable output bytes.
    unsafe { write_output(&value, output, capacity, out_len) }
}

/// # Safety
/// `out_len` must be writable, and a non-null `output` must have `capacity` writable bytes.
unsafe fn write_output(
    value: &str,
    output: *mut u8,
    capacity: u32,
    out_len: *mut u32,
) -> PluginStatus {
    let Ok(required) = u32::try_from(value.len()) else {
        return PluginStatus::LimitExceeded;
    };
    // SAFETY: The host provides a valid out_len pointer.
    unsafe { *out_len = required };
    if output.is_null() && capacity == 0 {
        return PluginStatus::Ok;
    }
    if capacity < required {
        return PluginStatus::LimitExceeded;
    }
    if required != 0 {
        if output.is_null() {
            return PluginStatus::InvalidArgument;
        }
        // SAFETY: The host provides at least `capacity` writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), output, required as usize) };
    }
    PluginStatus::Ok
}

impl LyricsApi {
    pub fn register<F>(&self, transform: F) -> Result<Resource, Error>
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        let mut callback: Box<Box<Transform>> = Box::new(Box::new(transform));
        let data = LyricsTransformerDataV2 {
            on_transform: Some(transform_line),
            callback_data: (&mut *callback as *mut Box<Transform>).cast(),
            ..Default::default()
        };
        self.register_data(&data, callback)
    }

    /// Registers a transformer whose output is shown below each line, such as a translation.
    ///
    /// The host calls it after the original lyrics are displayed, one line at a time in song
    /// order; return an empty string to leave a line without secondary text.
    pub fn register_secondary<F>(&self, transform: F) -> Result<Resource, Error>
    where
        F: Fn(&LyricsLine<'_>) -> String + Send + Sync + 'static,
    {
        let mut callback: Box<Box<SecondaryTransform>> = Box::new(Box::new(transform));
        let data = LyricsTransformerDataV2 {
            flags: LYRICS_TRANSFORMER_FLAG_SECONDARY,
            on_transform: Some(transform_secondary),
            callback_data: (&mut *callback as *mut Box<SecondaryTransform>).cast(),
            ..Default::default()
        };
        self.register_data(&data, callback)
    }

    fn register_data(
        &self,
        data: &LyricsTransformerDataV2,
        callback: Box<dyn std::any::Any + Send + Sync>,
    ) -> Result<Resource, Error> {
        let table = self
            ._host
            .query::<LyricsTransformApiV2>(abi::IFACE_LYRICS_TRANSFORM)?;
        let register = table
            .register
            .ok_or(Error::MissingFunction("lyrics.register"))?;
        let mut id = ResourceId::INVALID;
        // SAFETY: The callback pointer remains stable in Resource until release completes.
        success(unsafe { register(table.prefix.context, self._host.token, data, &mut id) })?;
        Ok(Resource {
            host: self._host.clone(),
            id,
            kind: ResourceKind::Lyrics,
            callback: Some(callback),
        })
    }
}

impl SettingsApi {
    pub fn create_page(&self, key: &str, title: &str) -> Result<Resource, Error> {
        let table = self._host.query::<SettingsApiV2>(abi::IFACE_SETTINGS)?;
        let create = table
            .create
            .ok_or(Error::MissingFunction("settings.create"))?;
        let mut data = SettingsPageDataV2::default();
        copy_fixed(&mut data.key, key);
        copy_fixed(&mut data.title, title);
        let mut item = SettingsItemV2 {
            kind: SETTINGS_ITEM_SECTION,
            ..Default::default()
        };
        copy_fixed(&mut item.label, title);
        data.items = &item;
        data.item_count = 1;
        let mut id = ResourceId::INVALID;
        // SAFETY: The host copies the page data during this call.
        success(unsafe { create(table.prefix.context, self._host.token, &data, &mut id) })?;
        Ok(Resource {
            host: self._host.clone(),
            id,
            kind: ResourceKind::Settings,
            callback: None,
        })
    }

    pub fn create_label_page(
        &self,
        key: &str,
        title: &str,
        label: &str,
    ) -> Result<Resource, Error> {
        let table = self._host.query::<SettingsApiV2>(abi::IFACE_SETTINGS)?;
        let create = table
            .create
            .ok_or(Error::MissingFunction("settings.create"))?;
        let mut item = SettingsItemV2 {
            kind: SETTINGS_ITEM_SECTION,
            ..Default::default()
        };
        copy_fixed(&mut item.label, label);
        let mut data = SettingsPageDataV2::default();
        copy_fixed(&mut data.key, key);
        copy_fixed(&mut data.title, title);
        data.items = &item;
        data.item_count = 1;
        let mut id = ResourceId::INVALID;
        // SAFETY: The item and page remain valid until the host copies them.
        success(unsafe { create(table.prefix.context, self._host.token, &data, &mut id) })?;
        Ok(Resource {
            host: self._host.clone(),
            id,
            kind: ResourceKind::Settings,
            callback: None,
        })
    }
}

impl TextApi {
    pub fn measure(&self, text: &str, size: f32, family: &str) -> Result<TextMetricsV2, Error> {
        let mut style = super::TextStyle::default_at(size);
        style.family = family.to_owned();
        self.measure_style(text, &style)
    }

    pub fn measure_style(
        &self,
        text: &str,
        style: &super::TextStyle,
    ) -> Result<TextMetricsV2, Error> {
        let table = self._host.query::<TextApiV2>(abi::IFACE_TEXT)?;
        let measure = table
            .measure
            .ok_or(Error::MissingFunction("text.measure"))?;
        let style = TextStyleV2 {
            size: style.size,
            weight: style.weight,
            italic: u8::from(style.italic),
            reserved: 0,
            family: Utf8Slice::borrowed(&style.family),
        };
        let mut metrics = TextMetricsV2::default();
        // SAFETY: Borrowed text and style remain valid during the host call.
        success(unsafe {
            measure(
                table.prefix.context,
                self._host.token,
                Utf8Slice::borrowed(text),
                &style,
                &mut metrics,
            )
        })?;
        Ok(metrics)
    }
}

impl ImageApi {
    pub fn decode(&self, encoded: &[u8]) -> Result<ImageHandle, Error> {
        let table = self.0.query::<ImageApiV2>(abi::IFACE_IMAGE)?;
        let decode = table.decode.ok_or(Error::MissingFunction("image.decode"))?;
        let mut id = ImageId::INVALID;
        // SAFETY: The borrowed image bytes remain valid during the call.
        success(unsafe {
            decode(
                table.prefix.context,
                self.0.token,
                ByteSlice::borrowed(encoded),
                &mut id,
            )
        })?;
        Ok(ImageHandle {
            host: self.0.clone(),
            id,
        })
    }

    pub fn upload_rgba(&self, width: u32, height: u32, rgba: &[u8]) -> Result<ImageHandle, Error> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(rgba.len()) {
            return Err(Error::Length);
        }
        let table = self.0.query::<ImageApiV2>(abi::IFACE_IMAGE)?;
        let upload = table
            .upload_rgba
            .ok_or(Error::MissingFunction("image.upload_rgba"))?;
        let mut id = ImageId::INVALID;
        // SAFETY: The borrowed RGBA bytes remain valid during the call.
        success(unsafe {
            upload(
                table.prefix.context,
                self.0.token,
                width,
                height,
                ByteSlice::borrowed(rgba),
                &mut id,
            )
        })?;
        Ok(ImageHandle {
            host: self.0.clone(),
            id,
        })
    }
}

impl StoreApi {
    pub fn set(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        let table = self._host.query::<StoreApiV2>(abi::IFACE_STORE)?;
        let set = table.set.ok_or(Error::MissingFunction("store.set"))?;
        // SAFETY: The host copies both borrowed slices before returning.
        success(unsafe {
            set(
                table.prefix.context,
                self._host.token,
                Utf8Slice::borrowed(key),
                ByteSlice::borrowed(value),
            )
        })
    }

    pub fn delete(&self, key: &str) -> Result<(), Error> {
        let table = self._host.query::<StoreApiV2>(abi::IFACE_STORE)?;
        let delete = table.delete.ok_or(Error::MissingFunction("store.delete"))?;
        // SAFETY: The borrowed key remains valid during the call.
        success(unsafe {
            delete(
                table.prefix.context,
                self._host.token,
                Utf8Slice::borrowed(key),
            )
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let table = self._host.query::<StoreApiV2>(abi::IFACE_STORE)?;
        let get = table.get.ok_or(Error::MissingFunction("store.get"))?;
        let mut required = 0;
        let mut found = 0;
        // SAFETY: Output scalars remain writable during this synchronous call.
        let status = unsafe {
            get(
                table.prefix.context,
                self._host.token,
                Utf8Slice::borrowed(key),
                std::ptr::null_mut(),
                0,
                &mut required,
                &mut found,
            )
        };
        if status != PluginStatus::Ok && status != PluginStatus::LimitExceeded {
            return Err(super::status_error(status));
        }
        if found == 0 {
            return Ok(None);
        }
        let mut value = vec![0; required as usize];
        if required == 0 {
            return Ok(Some(value));
        }
        // SAFETY: The allocation has `required` writable bytes.
        success(unsafe {
            get(
                table.prefix.context,
                self._host.token,
                Utf8Slice::borrowed(key),
                value.as_mut_ptr(),
                required,
                &mut required,
                &mut found,
            )
        })?;
        value.truncate(required as usize);
        Ok(if found == 0 { None } else { Some(value) })
    }
}
