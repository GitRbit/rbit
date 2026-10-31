#!/usr/bin/env python3
"""Build reproducible, data-only example packs from the committed host schema."""
import argparse, hashlib, io, json, re, zipfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / 'crates/rbit-messages/src/catalog.json'
SPANISH = {
 'Explore':'Explorar', 'Sign in':'Iniciar sesión', 'Sign out':'Cerrar sesión', 'Sign up':'Registrarse',
 'Create an account':'Crear una cuenta', 'Skip to content':'Saltar al contenido', 'Main':'Principal',
 'Settings':'Configuración', 'Site administration':'Administración del sitio', 'Dashboard':'Panel',
 'Profile':'Perfil', 'Issues':'Incidencias', 'Pull requests':'Solicitudes de cambios', 'Themes':'Temas',
 'Light mode':'Modo claro', 'Dark mode':'Modo oscuro', 'Notifications':'Notificaciones',
 'New repository':'Nuevo repositorio', 'New organization':'Nueva organización', 'Language':'Idioma',
 'Choose the language for your account or this browser.':'Elige el idioma para tu cuenta o este navegador.',
 'Automatic (browser or instance default)':'Automático (navegador o valor del sitio)', 'Save language':'Guardar idioma',
 'Save':'Guardar', 'Cancel':'Cancelar', 'Delete':'Eliminar', 'Edit':'Editar', 'Create':'Crear', 'Search':'Buscar',
 'Name':'Nombre', 'Description':'Descripción', 'Username':'Nombre de usuario', 'Password':'Contraseña',
 'Email':'Correo electrónico', 'Username or email':'Nombre de usuario o correo electrónico',
 'Repositories':'Repositorios', 'Users':'Usuarios', 'Organizations':'Organizaciones', 'Public':'Público',
 'Private':'Privado', 'Actions':'Actions', 'Workflows':'Flujos de trabajo', 'Runners':'Ejecutores',
 'Success':'Éxito', 'Failure':'Error', 'Cancelled':'Cancelado', 'Skipped':'Omitido', 'Queued':'En cola',
 'In progress':'En curso', 'Waiting':'En espera', 'Awaiting approval':'Pendiente de aprobación',
 'Plugins':'Complementos', 'Languages':'Idiomas', 'Enable languages':'Activar idiomas',
 'Disable languages':'Desactivar idiomas', 'Install language pack':'Instalar paquete de idioma',
 'Remove pack':'Eliminar paquete', 'Footer':'Pie de página', 'Privacy':'Privacidad', 'Terms':'Condiciones',
 'Support':'Ayuda', 'Status':'Estado', 'Enabled':'Activado', 'Disabled':'Desactivado', 'Enabled by default':'Activado de forma predeterminada',
 'Close':'Cerrar', 'Filter':'Filtrar', 'Clear':'Limpiar', 'Jobs':'Trabajos', 'Run workflow':'Ejecutar flujo de trabajo',
}
ACCENTS = str.maketrans('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz', 'ȦƁƇḒḖƑƓĦĪĴĶĿḾȠǾƤɊŘŞŦŬṼẆẊẎẐȧƀƈḓḗƒɠħīĵķŀḿƞǿƥɋřşŧŭṽẇẋẏẑ')
def quoted(text):
    # Fluent string literals support Unicode escapes, not JSON's short control escapes.
    return json.dumps(text, ensure_ascii=False).replace('\\n', '\\u000A').replace('\\r', '\\u000D').replace('\\t', '\\u0009')
def value(message, locale):
    text = message['english']
    if message['key'] == 'language-pack-count':
        if locale == 'es': return '{ $count ->\n    [one] Una lengua instalada\n   *[other] { $count } lenguas instaladas\n    }'
        if locale == 'ar-XB': return '{ $count ->\n    [one] حزمة لغة واحدة\n   *[other] حزم اللغات: { $count }\n    }'
        return '{ $count ->\n    [one] [Ǿƞḗ ŀȧƞɠŭȧɠḗ ƥȧƈķ]\n   *[other] [Īƞşŧȧŀŀḗḓ ŀȧƞɠŭȧɠḗ ƥȧƈķş: { $count }]\n    }'
    if message.get('args'):
        if locale == 'es':
            if message['key'] != 'language-pack-coverage': return None
            text = '{count} / {total} mensajes validados con el esquema de instalación. Los mensajes que faltan se muestran en inglés.'
        chunks=[]
        for part in re.split(r'(\{[a-zA-Z][a-zA-Z0-9_-]*\})',text):
            if part.startswith('{') and part.endswith('}') and part[1:-1] in message['args']: chunks.append('{ $'+part[1:-1]+' }')
            elif part:
                if locale == 'en-XA': part=part.translate(ACCENTS)
                elif locale == 'ar-XB': part='اختبار '+part.translate(ACCENTS)
                chunks.append('{ '+quoted(part)+' }')
        return ''.join(chunks)
    if locale == 'es':
        translated = SPANISH.get(text)
        if translated is None:return None
    elif locale == 'en-XA': translated = '[ '+text.translate(ACCENTS)+' '+'·' * max(4,len(text)//3)+' ]'
    else: translated = 'اختبار [ '+text.translate(ACCENTS)+' ] نهاية'
    return '{ '+quoted(translated)+' }'
def build(locale, messages):
    files = {'LICENSE.txt': b'Example language data: MIT OR Apache-2.0. See repository LICENSE-MIT and LICENSE-APACHE.\n'}
    catalogs = {}
    for namespace in ['core','actions']:
        lines=[]
        for message in messages:
            if message['namespace'] != namespace:continue
            translated=value(message,locale)
            if translated is not None:lines.append(message['key']+' = '+translated+'\n')
        if lines:
            path='catalogs/'+namespace+'.ftl'; catalogs[namespace]=path;files[path]=''.join(lines).encode()
    manifest='schema = 1\n'+''.join(k+' = '+quoted(v)+'\n' for k,v in {
      'id':'language.'+locale.lower(),'version':'1.0.0','locale':locale,'direction':'rtl' if locale=='ar-XB' else 'ltr',
      'message_schema':'^1.0','publisher':'Rbit example language data','license':'MIT OR Apache-2.0','license_file':'LICENSE.txt'}.items())
    manifest+='\n[catalogs]\n'+''.join(k+' = '+quoted(v)+'\n' for k,v in catalogs.items())
    manifest+='\n[checksums]\n'+''.join(quoted(k)+' = '+quoted(hashlib.sha256(v).hexdigest())+'\n' for k,v in sorted(files.items()))
    files['pack.toml']=manifest.encode()
    output=io.BytesIO()
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for name,content in sorted(files.items()):
            info=zipfile.ZipInfo(name,date_time=(2026,1,1,0,0,0));info.create_system=3;info.external_attr=0o100644<<16;info.compress_type=zipfile.ZIP_DEFLATED
            z.writestr(info,content,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    return output.getvalue()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--check',action='store_true');args=parser.parse_args()
    messages=json.loads(SCHEMA.read_text())
    for locale in ['es','en-XA','ar-XB']:
        path=ROOT/'language-packs'/(locale+'.rbit-language');data=build(locale,messages)
        if args.check:
            if not path.exists() or path.read_bytes()!=data:raise SystemExit(str(path.relative_to(ROOT))+': regenerate example language packs')
        else:path.write_bytes(data)
        print(str(path.relative_to(ROOT))+': '+str(len(data))+' bytes')
if __name__=='__main__':main()
