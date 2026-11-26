El conocimiento a bajo nivel de redes TCP/IP siempre ha sido mi punto
débil. Por lo general, para mí, una conexión TCP es un flujo de
**datos** entre dos extremos identificados por su IP y su puerto, lo
que pasa por debajo muchas veces se me escapa.

La razón principal es que no me da la vida para todo, pero otro de los
motivos es que pocas veces tienes que preocuparte de esas
capas. Principalmente porque las implementaciones están a prueba de
bombas (literalmente). Buena parte del código que implementa estos
protocolos vive en el espacio del kernel, y yo ahí habré tocado, en
toda mi vida, una o ninguna línea de código (hint: ninguna, es
ninguna).

Le pregunto a ChatGPT cuantas veces se ejecutan por segundo en el
mundo esas líneas de código, solo linux; me dice que 1000 billones de
veces por segundo, billones españoles, esto en inglés sería 1
quadrillion, que es el típico número que dejaste de nombrar cuando
cumpliste cierta edad. Disclaimer: Pensándolo un poco más fuerte,
sospecho que el margen de error de esta estimación es, como mínimo,
del mismo orden de magnitud que el número estimado.

Recientemente, me encontré con un problema que requería tocar la
implementación de TCP/IP a bajo nivel. El problema era sencillo,
estábamos usando interfaces TUN para conectar equipos con
wireguard, pero para hacer eso, necesitábamos permisos de
`CAP_NET_ADMIN`. Teniendo el cuenta el caso de uso; que era
simplemente acceder a servicios internos desde una ip virtual, la
implementación podía ser 100% en espacio de usuario y ahí que me metí.

## El problema, o; que cojones es una pila TCP/IP en espacio de usuario y
por que diablos yo necesito una

En (kittengrid)[https://kittengrid.com] tenemos un
(agente)[https://github.com/kittengrid/agent] que permite manejar
servicios que corren internamente y exportarlos a través de nuestra
infraestructura. Para hacer esto, el proyecto utiliza una librería en
Rust que usa a su vez interfaces TUN de Linux. Voy a hablar primero
un poco de wireguard para que se entienda.

### WireGuard

WireGuard permite conectar dos o más *peers* a través de una red no
confiable, utilizando UDP para transportar paquetes IP de forma
segura.

Una forma habitual de utilizarlo, especialmente con implementaciones
en espacio de usuario, es mediante interfaces TUN. Una interfaz TUN no
es más que una interfaz de red virtual proporcionada por el kernel de
Linux que permite a un programa en espacio de usuario recibir y enviar
paquetes IP en crudo.

Volviendo a WireGuard, lo que hace es cifrar y encapsular esos
paquetes IP para enviarlos a través de UDP y, en el otro extremo,
descifrarlos y entregarlos de nuevo. De esta forma, lo que envías por
la interfaz de un *peer* aparece en la interfaz del otro como si
hubiera llegado a través de una interfaz de red convencional.

Pero aquí está lo importante: WireGuard se encarga de transportar los
paquetes IP, no de procesar las conexiones TCP que contienen. Cuando
utilizamos una interfaz TUN, los paquetes recibidos se entregan al
kernel de Linux, que se encarga de procesarlos mediante su propia pila
TCP/IP.

Nótese que el kernel también soporta wireguard directamente, con lo
que podríamos incluso montarlo todo en espacio de kernel y obviar la
parte de interfaces TUN, pero si quieres usar una implementación de
Wireguard en espacio de usuario, hacer uso de interfaces TUN
simplifica bastante el problema. A continuación voy a explorar tres
alternativas diferentes:
- WireGuard 100% en espacio de kernel
- WireGuard en espacio de usuario utilizando la pila TCP/IP del kernel
  a través de una interfaz TUN.
- WireGuard 100% en espacio de usuario.

### WireGuard en espacio del kernel

Este es el caso más simple, todo pasa en espacio del kernel y los
procesos solo ven sockets.

<!-- imagen !-->

En el diagrama vemos claramente cómo se realiza la comunicación. Para
nuestra aplicación, sigue existiendo mi abstracción favorita: un flujo
de **datos** entre dos extremos identificados por su IP y su puerto, es
decir, un socket. En resumen, un proceso en A, puede intercambiar
datos con un servicio escuchando en el puerto 8080 en B.

Por debajo, la cosa se complica un poco. Los paquetes que envía el
proceso en A hacia B entran en el kernel a través del socket, que no
es más que una abstracción que permite a las aplicaciones de usuario
interactuar con la pila de red. A continuación, esos datos pasan por
la pila TCP/IP, que construye los paquetes correspondientes y decide
enviarlos por la interfaz virtual wg0 según las rutas de enrutamiento
configuradas.

Una vez el tráfico se enruta hacia esa interfaz, entra en juego
WireGuard en espacio de kernel. WireGuard cifra el paquete IP original
y lo encapsula dentro de otro paquete IP que utiliza UDP como
transporte. Ese paquete encapsulado se envía finalmente a través de
una interfaz de red física hacia el servidor B.

En el servidor B, por su parte, hay un proceso escuchando en el
puerto 8080. Ese proceso ya le ha indicado previamente al kernel,
mediante otro socket, que quiere recibir los datos que lleguen a ese
puerto.

Cuando el paquete UDP procedente del servidor A llega a la interfaz de
red física del servidor B, el kernel realiza el proceso
inverso. WireGuard recibe el datagrama UDP, lo desencapsula y lo
descifra, reinyectando el paquete IP original en la pila TCP/IP. Esta
termina entregando los datos al socket que estaba escuchando en el
puerto 8080, aquí WireGuard ya no ha hecho nada, es un paquete IP
normal que es procesado por la pila de red del kernel.

Finalmente, esos datos llegan al proceso de usuario. Con este esquema,
desde la aplicación solo se han enviado y recibido **datos**: en ningún
momento los procesos han tenido que construir manualmente un paquete
IP. Esta característica se hará más evidente en los ejemplos
siguientes.

### Implementación actual, usando interfaces TUN
Este caso nos interesa especialmente, porque es justamente el estado
en el que estaba nuestro [agente](https://github.com/kittengrid/agent)
cuando decidimos movernos a un enfoque 100 % en espacio de usuario.

<!-- imagen -->

Aquí, la parte del servidor A no cambia: todo sigue funcionando en
espacio de kernel, con las ventajas de rendimiento que eso supone. Por
su lado, en el servidor B aparece un nuevo elemento: una interfaz TUN.

Una interfaz TUN es uno de los mecanismos que nos proporciona el
kernel de Linux para trabajar directamente con paquetes IP desde
espacio de usuario, que es justamente lo que queremos hacer. Cuando
levantas una interfaz TUN, lo que estás haciendo es, básicamente,
asociar una interfaz de red virtual a un descriptor de fichero.

En esencia, estás permitiendo que un proceso de usuario reciba y envíe
paquetes IP utilizando las llamadas al sistema `read()` y `write()`. O
sea, ya no haces primero un `socket()` y después un `write()` para
empezar a enviar información. Ahora haces un `open()`, configuras la
interfaz mediante `ioctl()` y después puedes hacer un `write()`.

Pero aquí hay una diferencia importante: ya no mandas **datos** (que
era lo que pasaba antes con los sockets TCP), ahora mandas **paquetes
IP**.

Y esta distinción es fundamental, como veremos más adelante.

De esta manera, adaptándolo a nuestro ejemplo, el recorrido del
tráfico en el servidor B cambia ligeramente respecto al caso anterior.

Cuando el paquete UDP cifrado llega a la interfaz de red física del
servidor B, el kernel lo procesa mediante su pila UDP/IP y lo entrega
a un socket UDP que tiene abierto nuestro proceso de WireGuard.

El proceso, que estaba esperando recibir datos en ese socket, obtiene
el datagrama mediante una llamada como `recvfrom()` o
`recvmsg()`. Ahora sí, WireGuard se encarga de descifrarlo y
desencapsular el paquete IP original. 

¿Y qué hacemos con ese paquete IP?

Pues se lo devolvemos al kernel, haciendo un `write()` sobre el
descriptor de fichero asociado a la interfaz TUN. El kernel lo recibe
como si acabara de llegar por una interfaz de red y vuelve a
procesarlo mediante su pila TCP/IP.

Esto es equivalente, a grandes rasgos, a lo que hacía antes la
implementación de WireGuard en espacio de kernel, que entregaba el
paquete descifrado directamente a la pila de red. La diferencia es que
ahora lo hacemos mediante una llamada al sistema `write()`, mientras
que antes el código del kernel utilizaba sus propias funciones
internas, que voy a llamar `kwrite()` porque yo lo valgo.

NOTA: Me dicen por el ping**AI**nillo que en realidad:

> el kernel utiliza sk_buff para representar los paquetes y
> mecanismos internos para entregarlos a la pila de red, sin necesidad
> de pasar por descriptores de fichero ni realizar llamadas al sistema

Al igual que en el caso anterior, la pila termina entregando los datos
al socket de la conexión aceptada por el servicio que estaba
escuchando en el puerto 8080. Llegados a este punto, WireGuard ya ha
terminado su trabajo. Lo que tenemos es un paquete IP normal que el
kernel sabe perfectamente cómo procesar.

### 

El funcionamiento, a grandes rasgos, es sencillo: levantas una
interfaz TUN, la conectas a la implementación de WireGuard, configuras
el túnel, las IP y las rutas necesarias, y ya tienes todo el tinglado
montado.

Con esto, los procesos de ambos extremos pueden establecer conexiones
TCP sin enterarse de que, por debajo, hay una implementación de
WireGuard funcionando en espacio de usuario.

El proyecto estaba en Rust, la librería de wireguard que usábamos no
requería una interfaz TUN. La idea era dejar de requerir permisos
para lanzar el agente, que éste levantara y hiciera port forwarding a
los servicios locales que se levantaban.

Cuando conseguí que la librería de wireguard me mostrara paquetes entrantes
me encontré con la realidad en toda su crudeza. Lo que me venía de la
interfaz virtual eran, lógicamente, paquetes IP (aquí la idea es que
de fondo se escuche una música perturbadora).

La pregunta que me hice fue sencilla. Qué hago yo ahora con esto me
cago en mi puta bida?. La primera respuesta fue, bueno, pues yo que se,
me voy pillando los SYN que me lleguen y respondo con un SYN/ACK y
proceso el ACK, despues me monto un stream tcp en local hacia el servicio
que quiero exponer y voy pasando el payload de los paquetes de la
conexión hasta allá, y de ahí voy tirando, me monto una capa inicial que
vaya inspeccionando los paquetes y ver si son de algo que ya tengo, o
me llega otro SYN y monto otro stream. Parecía complicado. Básicamente
me tenía que implementar el protocolo TCP yo, en espacio de usuario.

Pensé, a ver, que esto se lo dices a un agente, te dice que tardarías
tipo 3 meses, y después te lo implementa en 5 minutos y unos cuantos
cientos de miles de tokens, el problema es que pero no quería tener
una pelota de código indescifrable en mi proyecto.

La solución fue simple, usar otra pelota de código indescifrable
llamada smolctp.

> smoltcp is a standalone, event-driven TCP/IP stack that is designed
> for bare-metal, real-time systems. Its design goals are simplicity
> and robustness. Its design anti-goals include complicated
> compile-time computations, such as macro or type tricks, even at
> cost of performance degradation.

## Smoltcp

Smoltcp está pensado para ser usado en entornos con recursos limitados
y cuenta con algunas optimizaciones que permiten limitar el overhead
de paso de datos.

Parecía que todo iba a ser sencillo, pero, no. Después de mirar la
librería y de repasar los ejemplos, terminé con más dudas que
respuestas. Sabía que en algún sitio de esa librería habría una cosa
que inspeccionaría un paquete IP y vería si es TCP, que seguramente en
alguna parte del código habría una parte que sabría cómo generar una
respuesta a un SYN, pero no era muy evidente como podía yo adaptar
todo esto a mi proyecto.

Para poder sumergirme en la librería, decidí montarme un proyecto
simple y poco a poco ir añadiéndole complejidad hasta llegar a mi
objetivo.
