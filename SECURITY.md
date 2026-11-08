# Política de seguridad

## Cómo reportar una vulnerabilidad

No abras un issue público para reportar un problema de seguridad.

Usa el aviso privado de GitHub: entra en la pestaña **Security** del repositorio,
elige **Report a vulnerability** y describe el problema con el detalle que
puedas. Ahí la conversación queda privada entre tú y el equipo.

Incluye, si es posible:

- Versión de EzAgent y de Windows.
- Pasos para reproducir el problema.
- Impacto que crees que tiene (por ejemplo, ejecución de comandos, exposición de
  claves, lectura de archivos).
- Logs relevantes, sin claves de API ni datos personales.

Revisamos los reportes y respondemos en un plazo razonable. No prometemos
tiempos concretos de respuesta ni de corrección, pero te mantenemos al tanto
mientras dure la investigación.

## Versiones soportadas

La serie **0.1.x** es la que recibe correcciones de seguridad. Las versiones
anteriores no se mantienen.

## Alcance

Estos son los límites de seguridad que el proyecto se compromete a respetar:

- **Ejecución de agentes.** El LLM nunca ejecuta comandos libres. Solo lanza
  CLIs de agentes declarados en la configuración, con los argumentos pasados
  como lista y sin shell intermedio.
- **Servidor IPC local.** Escucha únicamente en `127.0.0.1`. A partir de la
  etapa E5 exige un token por sesión.
- **Claves de API.** Nunca se escriben en los logs ni en la auditoría.
- **Salida de agentes.** El texto que devuelven los agentes, la web y los
  archivos se trata como dato no confiable: no se interpreta como instrucción
  para el propio EzAgent.

Quedan fuera de alcance los fallos que dependan de software de terceros
(whisper.cpp, Kokoro, el proveedor del LLM o Windows) cuando no haya una
configuración de EzAgent que los provoque o agrave.
