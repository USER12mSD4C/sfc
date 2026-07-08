use std::ffi::CStr;

fn get_group_name(gid: libc::gid_t) -> String {
    unsafe {
        let grp = libc::getgrgid(gid);
        if !grp.is_null() {
            CStr::from_ptr((*grp).gr_name)
                .to_string_lossy()
                .into_owned()
        } else {
            gid.to_string()
        }
    }
}

fn main() {
    unsafe {
        let egid = libc::getegid(); // Читаем первичную (эффективную) группу процесса
        let mut groups = [0; 1024];
        let num = libc::getgroups(groups.len() as libc::c_int, groups.as_mut_ptr());
        if num < 0 {
            std::process::exit(1);
        }

        let mut names = Vec::new();
        // Первичная группа по стандарту идет первой
        names.push(get_group_name(egid));

        for &gid in &groups[..num as usize] {
            if gid != egid {
                names.push(get_group_name(gid));
            }
        }
        println!("{}", names.join(" "));
    }
}
