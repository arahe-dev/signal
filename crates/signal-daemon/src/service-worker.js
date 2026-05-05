self.addEventListener('push', function(event) {
  console.log('Push event received:', event);

  const data = event.data ? event.data.json() : {};

  const options = {
    body: data.body || 'New Signal message. Tap to open inbox.',
    icon: '/icon-192.png',
    badge: '/icon-192.png',
    vibrate: [100, 50, 100],
    data: {
      url: data.url || '/'
    },
    actions: [
      { action: 'open', title: 'Open Inbox' }
    ]
  };

  event.waitUntil(
    self.registration.showNotification(data.title || 'Signal', options)
  );
});

self.addEventListener('notificationclick', function(event) {
  console.log('Notification click:', event);

  event.notification.close();
  const rawUrl = event.notification.data.url || '/';
  const targetUrl = new URL(rawUrl, self.location.origin).href;
  const appPrefix = self.location.origin + '/app';

  event.waitUntil(
    clients.matchAll({ type: 'window', includeUncontrolled: true }).then(function(clientList) {
      for (const client of clientList) {
        if ('focus' in client && client.url === targetUrl) {
          return client.focus();
        }
        if ('focus' in client && client.url.startsWith(appPrefix)) {
          return client.focus().then(function(focused) {
            focused.postMessage({ type: 'open-url', url: targetUrl });
            return focused;
          });
        }
      }
      return clients.openWindow(targetUrl);
    })
  );
});

self.addEventListener('message', function(event) {
  console.log('Service worker message:', event.data);
});
